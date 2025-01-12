use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;

use crate::db::{self, DBEntity, User, ZoneOwnership};
use crate::enums::{RecordClass, RecordType};
use crate::error::GoatNsError;
use crate::zones::{FileZone, ZoneRecord};
use sqlx::{Pool, Sqlite};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, instrument, warn};

type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
/// Commands that can be sent to the datastore
pub enum Command {
    /// Query a record from the database
    GetRecord {
        /// Reversed vec of the name
        name: Vec<u8>,
        /// The type of record to get
        rrtype: RecordType,
        /// The class of record to get
        rclass: RecordClass,
        /// The response channel
        resp: Responder<Option<ZoneRecord>>,
    },
    /// Query a zone from the database
    GetZone {
        /// If you know the ID supply it
        id: Option<i64>,
        /// If you know the name supply it
        name: Option<String>,
        /// The response channel
        resp: Responder<Option<FileZone>>,
    },
    /// Query a list of zones from the database
    GetZoneNames {
        /// Filter by user
        user: User,
        /// The offset to start at
        offset: i64,
        /// The number of records to return
        limit: i64,
        /// The response channel
        resp: Responder<Vec<FileZone>>,
    },
    /// Import a file directly into the database
    ImportFile {
        /// Filename to load
        filename: String,
        /// If you only want to import a single zone, specify the name
        zone_name: Option<String>,
        /// The response channel
        resp: Responder<()>,
    },
    /// Shutdown the datastore
    Shutdown,
    /// Create a new zone
    CreateZone {
        /// Zone data
        zone: FileZone,

        /// Zone ownership
        userid: i64,

        /// The response channel
        resp: Responder<FileZone>,
    },
    /// Delete a zone
    DeleteZone,
    /// update a zone
    UpdateZone,
    /// Delete user
    DeleteUser,
    /// Create a new user
    CreateUser {
        /// Username
        username: String,
        /// Reference from OIDC
        authref: String,
        /// Is this user an admin?
        admin: bool,
        /// Is this user disabled?
        disabled: bool,
        /// The response channel
        resp: Responder<bool>,
    },
    /// Get a user
    GetUser {
        /// Database ID if you have it
        id: Option<i64>,
        /// username if you have it
        username: Option<String>,
        /// The response channel
        resp: Responder<()>,
    },
    /// Update a user
    UpdateUser,
    /// Remove zone ownership (optionally include a user)
    DeleteOwnership {
        /// Zone ID
        zoneid: i64,
        /// User ID (db ID)
        userid: Option<i64>,
        /// The response channel
        resp: Responder<()>,
    },
    /// Get ownership of a zone, or zones for a user
    GetOwnership {
        /// Zone ID
        zoneid: Option<i64>,
        /// User ID
        userid: Option<i64>,
        /// The response channel
        resp: Responder<Vec<Arc<ZoneOwnership>>>,
    },
    /// Create ownership of a zone
    PostOwnership {
        /// Zone ID
        zoneid: i64,
        /// User ID
        userid: i64,
        /// The response channel
        resp: Responder<ZoneOwnership>,
    },
}

async fn handle_get_command(
    // database pool
    conn: &Pool<Sqlite>,
    // this is the result from the things in memory
    // zone_get: Option<&ZoneRecord>,
    name: Vec<u8>,
    rrtype: RecordType,
    rclass: RecordClass,
    resp: oneshot::Sender<Option<ZoneRecord>>,
) -> Result<(), String> {
    debug!(
        "query name={:?} rrtype={rrtype:?} rclass={rclass}",
        from_utf8(&name).unwrap_or("-"),
    );

    // query the database
    let db_name =
        from_utf8(&name).map_err(|e| format!("Failed to convert name to utf8 - {e:?}"))?;

    let mut zr = ZoneRecord {
        name: name.clone(),
        typerecords: vec![],
    };

    match db::get_records(conn, db_name.to_string(), rrtype, rclass, true).await {
        Ok(value) => zr.typerecords.extend(value),
        Err(err) => {
            error!("Failed to query db: {err:?}")
        }
    };

    // if let Some(value) = zone_get {
    //     // check if the type we want is in there, and only return the matching records
    //     let res: Vec<InternalResourceRecord> = value
    //         .to_owned()
    //         .typerecords
    //         .into_iter()
    //         .filter(|r| r == &rrtype && r == &rclass)
    //         .collect();
    //     zr.typerecords.extend(res);
    // };

    let result = match zr.typerecords.is_empty() {
        true => None,
        false => Some(zr),
    };

    if let Err(error) = resp.send(result) {
        debug!("error sending response from data store: {:?}", error)
    };
    Ok(())
}

/// Import a file directly into the database. Normally, you shouldn't use this directly, call it through calls to the datastore.
#[instrument(level = "info", skip(pool))]
pub async fn handle_import_file(
    pool: &Pool<Sqlite>,
    filename: String,
    zone_name: Option<String>,
) -> Result<(), GoatNsError> {
    let zones: Vec<FileZone> = crate::zones::load_zones(&filename)?;

    let zones = match zone_name {
        Some(name) => zones.into_iter().filter(|z| z.name == name).collect(),
        None => zones,
    };

    if zones.is_empty() {
        warn!("No zones to import!");
        return Err(GoatNsError::EmptyFile);
    } else {
        debug!("Starting import process");
    }

    let mut txn = pool.begin().await?;
    for zone in zones {
        let _saved_zone = zone
            .save_with_txn(&mut txn)
            .await
            .inspect_err(|err| error!("Failed to save zone {}: {err:?}", zone.name))?;
        info!("Imported {}", zone.name);
    }
    txn.commit()
        .await
        .inspect_err(|err| error!("Failed to commit transaction! {:?}", err))?;
    info!("Completed import process");
    Ok(())
}

#[instrument(level = "debug", skip(tx, pool))]
async fn handle_get_zone(
    tx: oneshot::Sender<Option<FileZone>>,
    pool: &Pool<Sqlite>,
    id: Option<i64>,
    name: Option<String>,
) -> Result<(), GoatNsError> {
    let mut txn = pool.begin().await?;
    let zone = crate::db::get_zone_with_txn(&mut txn, id, name).await?;
    drop(txn);

    tx.send(zone).map_err(|e| {
        GoatNsError::SendError(format!("Failed to send response on tokio channel: {:?}", e))
    })
}

async fn handle_get_zone_names(
    user: User,
    tx: oneshot::Sender<Vec<FileZone>>,
    pool: &Pool<Sqlite>,
    offset: i64,
    limit: i64,
) -> Result<(), GoatNsError> {
    let mut txn = pool.begin().await?;

    debug!("handle_get_zone_names: user={user:?}");
    let zones = user.get_zones_for_user(&mut txn, offset, limit).await?;

    debug!("handle_get_zone_names: {zones:?}");
    tx.send(zones).map_err(|e| {
        GoatNsError::SendError(format!("Failed to send response on tokio channel: {:?}", e))
    })
}

#[instrument(level = "info", skip(connpool))]
pub(crate) async fn handle_message(cmd: Command, connpool: &Pool<Sqlite>) -> Result<(), String> {
    match cmd {
        Command::GetZone { id, name, resp } => {
            let res = handle_get_zone(resp, connpool, id, name).await;
            if let Err(e) = res {
                error!("{e:?}")
            };
        }
        Command::GetZoneNames {
            resp,
            user,
            offset,
            limit,
        } => {
            let res = handle_get_zone_names(user, resp, connpool, offset, limit).await;
            if let Err(e) = res {
                error!("{e:?}")
            };
        }
        Command::Shutdown => {
            #[cfg(test)]
            println!("### Datastore was sent shutdown message, shutting down.");
            info!("Datastore was sent shutdown message, shutting down.");
            return Err("Datastore was sent shutdown message, shutting down.".to_string());
        }
        Command::ImportFile {
            filename,
            resp,
            zone_name,
        } => {
            handle_import_file(connpool, filename, zone_name)
                .await
                .map_err(|e| format!("{e:?}"))?;
            match resp.send(()) {
                Ok(_) => info!("DS Sent Success"),
                Err(err) => {
                    let res = format!("Failed to send response: {err:?}");
                    info!("{res}");
                }
            }
        }
        Command::GetRecord {
            name,
            rrtype,
            rclass,
            resp,
        } => {
            let res = handle_get_command(connpool, name, rrtype, rclass, resp).await;
            if let Err(e) = res {
                error!("{e:?}")
            };
        }
        Command::CreateZone { zone, userid, resp } => {
            match zone.save(connpool).await {
                Ok(zone) => {
                    // TODO: create the ownership
                    if let Some(zoneid) = zone.id {
                        ZoneOwnership {
                            id: None,
                            userid,
                            zoneid,
                        }
                        .save(connpool)
                        .await
                        .map_err(|e| format!("{e:?}"))?;
                    }

                    if let Err(err) = resp.send(*zone.clone()) {
                        error!("Failed to send message back to caller after creating zone {zone:?}: {err:?}");
                    } else {
                        info!("Created zone: {:?}", zone);
                    }
                }
                Err(err) => {
                    error!("Failed to create zone: {zone:?} {err:?}");
                }
            };
        }
        Command::DeleteZone => {
            error!("Unimplemented: Command::DeleteZone")
        }
        Command::UpdateZone => {
            error!("Unimplemented: Command::PatchZone")
        }
        Command::CreateUser {
            username,
            authref,
            admin,
            disabled,
            resp,
        } => {
            let new_user = User {
                username: username.clone(),
                authref: Some(authref.clone()),
                admin,
                disabled,
                ..Default::default()
            };
            debug!("Creating: {new_user:?}");
            let res = match new_user.save(connpool).await {
                Ok(_) => true,
                Err(error) => {
                    error!("Failed to create {username}: {error:?}");
                    false
                }
            };
            if let Err(error) = resp.send(res) {
                error!("Failed to send message back to caller: {error:?}");
            }
        }
        Command::DeleteUser => error!("Unimplemented: Command::DeleteUser"),
        Command::GetUser { .. } => error!("Unimplemented: Command::GetUser"),
        Command::UpdateUser => error!("Unimplemented: Command::PatchUser"),
        Command::DeleteOwnership { .. } => error!("Unimplemented: Command::DeleteOwnership"),
        Command::GetOwnership {
            zoneid: _,
            userid,
            resp,
        } => {
            if let Some(userid) = userid {
                match ZoneOwnership::get_all_user(connpool, userid).await {
                    Ok(zone) => {
                        if let Err(err) = resp.send(zone) {
                            error!("Failed to send zone_ownership response: {err:?}")
                        };
                    }
                    Err(err) => {
                        error!("Failed to get all zone_ownership for user {userid}: {err:?}")
                    }
                }
            } else {
                error!("Unmatched arm in getownership")
            }
        }
        Command::PostOwnership { .. } => {
            error!("Unimplemented command: Command::PostOwnership")
        }
    }
    Ok(())
}

/// Manages the datastore, waits for signals from the server instances and responds with data
pub async fn manager(
    mut rx: mpsc::Receiver<crate::datastore::Command>,
    connpool: Pool<Sqlite>,
    cron_db_cleanup_timer: Option<Duration>,
) -> Result<(), String> {
    if let Some(timer) = cron_db_cleanup_timer {
        debug!("Spawning DB cron cleanup task");
        tokio::spawn(db::cron_db_cleanup(connpool.clone(), timer, None));
    }

    while let Some(cmd) = rx.recv().await {
        if handle_message(cmd, &connpool).await.is_err() {
            break;
        };
    }
    #[cfg(test)]
    println!("### manager is done!");
    Ok(())
}

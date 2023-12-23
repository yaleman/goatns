use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;

use crate::db::{self, DBEntity, User, ZoneOwnership};
use crate::enums::{RecordClass, RecordType};
use crate::zones::{FileZone, ZoneRecord};
use log::debug;
use sqlx::{Pool, Sqlite};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
pub enum Command {
    GetRecord {
        /// Reversed vec of the name
        name: Vec<u8>,
        rrtype: RecordType,
        rclass: RecordClass,
        resp: Responder<Option<ZoneRecord>>,
    },
    GetZone {
        id: Option<i64>,
        name: Option<String>,
        resp: Responder<Option<FileZone>>,
    },
    GetZoneNames {
        user: User,
        offset: i64,
        limit: i64,
        resp: Responder<Vec<FileZone>>,
    },
    ImportFile {
        filename: String,
        zone_name: Option<String>,
        resp: Responder<()>,
    },
    Shutdown,

    PostZone,
    DeleteZone,
    PatchZone,

    DeleteUser,
    CreateUser {
        username: String,
        authref: String,
        admin: bool,
        disabled: bool,
        resp: Responder<bool>,
    },
    GetUser {
        id: Option<i64>,
        username: Option<String>,
        resp: Responder<()>,
    },
    PostUser,
    PatchUser,

    DeleteOwnership {
        zoneid: Option<i64>,
        userid: Option<i64>,
        resp: Responder<()>,
    },
    GetOwnership {
        zoneid: Option<i64>,
        userid: Option<i64>,
        resp: Responder<Vec<Arc<ZoneOwnership>>>,
    },
    PostOwnership {
        zoneid: i64,
        userid: i64,
        resp: Responder<ZoneOwnership>,
    },
    // TODO: create a setter when we're ready to accept updates
    // Set {
    //     name: Vec<u8>,
    //     rrtype: RecordType,
    // }
    // CreateZone {
    //     zone: FileZone,
    // }
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
            log::error!("Failed to query db: {err:?}")
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
pub async fn handle_import_file(
    pool: &Pool<Sqlite>,
    filename: String,
    zone_name: Option<String>,
) -> Result<(), String> {
    let mut txn = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;

    let zones: Vec<FileZone> = crate::zones::load_zones(&filename)?;

    let zones = match zone_name {
        Some(name) => zones.into_iter().filter(|z| z.name == name).collect(),
        None => zones,
    };

    if zones.is_empty() {
        log::warn!("No zones to import!");
        return Err("No zones to import!".to_string());
    }

    for zone in zones {
        let _saved_zone = match zone.save_with_txn(&mut txn).await {
            Err(err) => {
                let errmsg = format!("Failed to save zone {}: {err:?}", zone.name);
                log::error!("{errmsg}");
                return Err(errmsg);
            }
            Ok(val) => val,
        };
        log::info!("Imported {}", zone.name);
    }
    txn.commit().await.map_err(|e| {
        log::error!("Failed to commit transaction!");
        format!("Failed to commit transaction: {e:?}")
    })?;
    log::info!("Completed import process");
    Ok(())
}

async fn handle_get_zone(
    tx: oneshot::Sender<Option<FileZone>>,
    pool: &Pool<Sqlite>,
    id: Option<i64>,
    name: Option<String>,
) -> Result<(), String> {
    let mut txn = pool.begin().await.map_err(|e| format!("{e:?}"))?;

    let zone = crate::db::get_zone_with_txn(&mut txn, id, name)
        .await
        .map_err(|e| format!("{e:?}"))?;

    tx.send(zone).map_err(|e| format!("{e:?}"))
}

async fn handle_get_zone_names(
    user: User,
    tx: oneshot::Sender<Vec<FileZone>>,
    pool: &Pool<Sqlite>,
    offset: i64,
    limit: i64,
) -> Result<(), String> {
    let mut txn = pool.begin().await.map_err(|e| format!("{e:?}"))?;

    log::debug!("handle_get_zone_names: user={user:?}");
    let zones = user
        .get_zones_for_user(&mut txn, offset, limit)
        .await
        .map_err(|e| format!("{e:?}"))?;

    log::debug!("handle_get_zone_names: {zones:?}");
    tx.send(zones).map_err(|e| format!("{e:?}"))
}

/// Manages the datastore, waits for signals from the server instances and responds with data
pub async fn manager(
    mut rx: mpsc::Receiver<crate::datastore::Command>,
    connpool: Pool<Sqlite>,
    cron_db_cleanup_timer: Option<Duration>,
) -> Result<(), String> {
    if let Some(timer) = cron_db_cleanup_timer {
        log::debug!("Spawning DB cron cleanup task");
        tokio::spawn(db::cron_db_cleanup(connpool.clone(), timer, None));
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::GetZone { id, name, resp } => {
                let res = handle_get_zone(resp, &connpool, id, name).await;
                if let Err(e) = res {
                    log::error!("{e:?}")
                };
            }
            Command::GetZoneNames {
                resp,
                user,
                offset,
                limit,
            } => {
                let res = handle_get_zone_names(user, resp, &connpool, offset, limit).await;
                if let Err(e) = res {
                    log::error!("{e:?}")
                };
            }
            Command::Shutdown => {
                #[cfg(test)]
                println!("### Datastore was sent shutdown message, shutting down.");
                log::info!("Datastore was sent shutdown message, shutting down.");
                break;
            }
            Command::ImportFile {
                filename,
                resp,
                zone_name,
            } => {
                handle_import_file(&connpool, filename, zone_name)
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                match resp.send(()) {
                    Ok(_) => log::info!("DS Sent Success"),
                    Err(err) => {
                        let res = format!("Failed to send response: {err:?}");
                        log::info!("{res}");
                    }
                }
            }
            Command::GetRecord {
                name,
                rrtype,
                rclass,
                resp,
            } => {
                let res = handle_get_command(
                    &connpool, // zones.get(name.to_ascii_lowercase()),
                    name, rrtype, rclass, resp,
                )
                .await;
                if let Err(e) = res {
                    log::error!("{e:?}")
                };
            }
            Command::PostZone => todo!(),
            Command::DeleteZone => todo!(),
            Command::PatchZone => todo!(),
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
                log::debug!("Creating: {new_user:?}");
                let res = match new_user.save(&connpool).await {
                    Ok(_) => true,
                    Err(error) => {
                        log::error!("Failed to create {username}: {error:?}");
                        false
                    }
                };
                if let Err(error) = resp.send(res) {
                    log::error!("Failed to send message back to caller: {error:?}");
                }
            }
            Command::DeleteUser => todo!(),
            Command::GetUser { .. } => todo!(),
            Command::PostUser => todo!(),
            Command::PatchUser => todo!(),
            Command::DeleteOwnership { .. } => todo!(),
            Command::GetOwnership {
                zoneid: _,
                userid,
                resp,
            } => {
                if let Some(userid) = userid {
                    match ZoneOwnership::get_all_user(&connpool, userid).await {
                        Ok(zone) => {
                            if let Err(err) = resp.send(zone) {
                                log::error!("Failed to send zone_ownership response: {err:?}")
                            };
                        }
                        Err(_) => todo!(),
                    }
                } else {
                    log::error!("Unmatched arm in getownership")
                }
            }
            Command::PostOwnership { .. } => todo!(),
        }
    }
    #[cfg(test)]
    println!("### manager is done!");
    Ok(())
}

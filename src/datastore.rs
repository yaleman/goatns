use std::str::from_utf8;
use std::time::Duration;

use crate::db::{self, DBEntity, User, ZoneOwnership};
use crate::enums::{RecordClass, RecordType};
use crate::zones::{FileZone, FileZoneRecord, ZoneRecord};
use log::{debug, error};
use sqlx::{Acquire, Pool, Sqlite, SqlitePool};
use tokio::sync::mpsc;
use tokio::sync::oneshot::{self, Sender};

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

    CreateZone {
        zone_name: String,
        user: User,
        rname: String,
        resp: Responder<DataStoreResponse>,
    },
    DeleteZone {
        resp: Responder<DataStoreResponse>,
        user: User,
        id: i64,
    },
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
        resp: Responder<ZoneOwnership>,
    },
    CreateZoneOwnership {
        zoneid: i64,
        userid: i64,
        resp: Responder<ZoneOwnership>,
    },

    CreateZoneRecord {
        zoneid: i64,
        userid: i64,
        record: FileZoneRecord,
        resp: Responder<DataStoreResponse>,
    },
}
#[derive(Debug)]
pub enum DataStoreResponse {
    /// Returns the zone id
    Created(i64),
    AlreadyExists,
    Deleted,
    NotFound,
    Failure(String),
}

async fn handle_get_command(
    // database pool
    conn: &Pool<Sqlite>,
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
    let db_name = from_utf8(&name)
        .map_err(|e| format!("Failed to convert name to utf8 - {e:?}"))
        .unwrap();

    let mut zr = ZoneRecord {
        name: name.clone(),
        typerecords: vec![],
    };

    match db::get_records(conn, db_name, rrtype, rclass, true).await {
        Ok(value) => zr.typerecords.extend(value),
        Err(err) => {
            log::error!("Failed to query db: {err:?}")
        }
    };

    // TODO: why aren't we filtering the responses here?
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

    debug!("handle_get_zone: id={id:?} name={name:?}");

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
        tokio::spawn(db::cron_db_cleanup(connpool.clone(), timer));
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::GetZone { id, name, resp } => {
                if let Err(e) = handle_get_zone(resp, &connpool, id, name).await {
                    log::error!("{e:?}")
                };
            }
            Command::GetZoneNames {
                resp,
                user,
                offset,
                limit,
            } => {
                if let Err(e) = handle_get_zone_names(user, resp, &connpool, offset, limit).await {
                    log::error!("{e:?}")
                };
            }
            Command::CreateZoneRecord {
                record,
                zoneid,
                resp,
                userid,
            } => {
                if let Err(e) =
                    handle_create_zone_record(&connpool, userid, zoneid, resp, record).await
                {
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
                if let Err(e) = handle_get_command(
                    &connpool, // zones.get(name.to_ascii_lowercase()),
                    name, rrtype, rclass, resp,
                )
                .await
                {
                    log::error!("{e:?}")
                };
            }
            Command::CreateZone {
                zone_name,
                user,
                rname,
                resp,
            } => {
                handle_create_zone(&connpool, zone_name, user, rname, resp).await?;
            }
            Command::DeleteZone { resp, user, id } => {
                let res = match handle_delete_zone(&connpool, user, id).await {
                    Ok(_) => DataStoreResponse::Deleted,
                    Err(err) => DataStoreResponse::Failure(err),
                };
                if let Err(err) = resp.send(res) {
                    log::error!("Failed to send message back to caller: {:?}", err);
                }
            }
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
            Command::GetOwnership { zoneid, resp, .. } => {
                if let Some(zoneid) = zoneid {
                    let mut txn = connpool
                        .begin()
                        .await
                        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;

                    match ZoneOwnership::get(&mut txn, &zoneid).await {
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
            Command::CreateZoneOwnership { .. } => todo!(),
        }
    }
    #[cfg(test)]
    println!("### manager is done!");
    Ok(())
}

pub async fn handle_delete_zone(connpool: &SqlitePool, user: User, id: i64) -> Result<(), String> {
    // confirm the zone is owned by the user, or that the user is an admin
    let mut txn = connpool
        .begin()
        .await
        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;
    let zone = ZoneOwnership::get(&mut txn, &id)
        .await
        .map_err(|err| format!("Failed to query zone ownership for zone id {id}: {err:?}"))?;

    if zone.userid != user.id.unwrap() && !user.admin {
        log::error!(
            "User {:?} tried to delete zone {:?} but doesn't own it",
            user,
            zone.id
        );
        return Err("User doesn't have permission to delete this zone!".to_string());
    }

    // delete the zone
    let conn = txn.acquire().await.map_err(|err| {
        log::error!("Failed to acquire connection: {err:?}");
        "Failed to acquire connection".to_string()
    })?;

    let zonedata = FileZone::get_with_txn(conn, &zone.zoneid)
        .await
        .map_err(|err| {
            error!(
                "Failed to get filezone based on zone id ({}) we just got from the datastore: {:?}",
                zone.zoneid, err
            );
            "Unable to get zone data from database".to_string()
        })?;

    zonedata.delete_with_txn(conn).await.map_err(|err| {
        error!("Failed to delete zone: {:?}", err);
        "Failed to delete zone".to_string()
    })?;

    // TODO: do we delete the zone records here, or should the cascading ownership of the zone do that, or the filezone delete?

    txn.commit().await.map_err(|err| {
        error!("Failed to commit transaction: {:?}", err);
        "Failed to commit transaction".to_string()
    })?;

    Ok(())
}

pub async fn handle_create_zone(
    connpool: &SqlitePool,
    zone_name: String,
    user: User,
    rname: String,
    resp: Sender<DataStoreResponse>,
) -> Result<(), String> {
    debug!(
        "Got a CreateZone for zone_name: {}, user: {:?}",
        zone_name, user
    );
    // need to check if the zone already exists
    let mut txn = connpool
        .begin()
        .await
        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;

    let zone = match FileZone::get_by_name(&mut txn, &zone_name).await {
        Ok(zone) => {
            log::error!(
                "Zone already exists while user {:?} tried to create {}",
                user,
                zone_name
            );
            Some(zone)
        }
        Err(err) => {
            if let sqlx::Error::RowNotFound = err {
                None // this is fine
            } else {
                log::error!("Failed to query db: {err:?}");
                None
            }
        }
    };

    let result = match zone {
        None => {
            let new_zone = FileZone {
                name: zone_name.clone(),
                id: None,
                rname,
                serial: 0,
                refresh: 60,
                retry: 60,
                expire: 60,
                minimum: 60,
                records: Vec::new(),
            };
            match new_zone.save_with_txn(&mut txn).await {
                Err(err) => {
                    log::error!("Failed to save zone: {err:?}");
                    DataStoreResponse::Failure(format!("Failed to save zone: {err:?}"))
                }
                Ok(res) => {
                    // ensure the ownership is created
                    let ownership = ZoneOwnership {
                        id: None,
                        zoneid: res.id.unwrap(), // TODO: check for None here, or at least beforehand
                        userid: user.id.unwrap(), // TODO: check for None here, or at least beforehand
                    };
                    if let Err(err) = ownership.save_with_txn(&mut txn).await {
                        error!("Failed to save ownership: {err:?}");
                        DataStoreResponse::Failure(
                            "Failed to save ownership, had to roll back zone creation".to_string(),
                        )
                    } else {
                        log::info!("Created zone: {:?}, sending result to user", res);
                        match txn.commit().await {
                            Ok(_) => DataStoreResponse::Created(res.id.unwrap_or(-1)),
                            Err(err) => {
                                error!("failed to commit transaction: {:?}", err);
                                DataStoreResponse::Failure("Failed to commit database transaction, please contact an admin".to_string())
                            }
                        }
                    }
                }
            }
        }
        Some(_) => DataStoreResponse::AlreadyExists,
    };

    if let Err(error) = resp.send(result) {
        log::error!("Failed to send message back to caller from PostMessage: {error:?}");
    }
    Ok(())
}

pub(crate) async fn handle_create_zone_record(
    pool: &SqlitePool,
    userid: i64,
    zoneid: i64,

    resp: Responder<DataStoreResponse>,
    record: FileZoneRecord,
) -> Result<(), String> {
    let mut txn = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;

    debug!("handle_create_zone_record getting user");

    let user = User::get_with_txn(&mut txn, &userid)
        .await
        .map_err(|err| format!("Failed to get user: {err:?}"))?;

    debug!("handle_create_zone_record getting zone ownership");
    let ownership = ZoneOwnership::get_with_txn(&mut txn, &zoneid)
        .await
        .map_err(|err| format!("Failed to get ownership for zone id {}: {:?}", zoneid, err))?;

    if ownership.userid != userid && !user.admin {
        return Err("User doesn't own zone and isn't admin!".to_string());
    }

    // create the zone record
    let record = record
        .save_with_txn(&mut txn)
        .await
        .map_err(|err| format!("Failed to save zone record: {err:?}"))?;

    match record.id {
        Some(id) => {
            txn.commit().await.map_err(|err| {
                format!(
                    "Failed to commit transaction while saving {:?}: {:?}",
                    record, err
                )
            })?;
            resp.send(DataStoreResponse::Created(id)).map_err(|err| {
                format!("Failed to send response to user after saving FZR with id {id}: {err:?}")
            })
        }
        None => Err("Failed to save record, no ID returned!".to_string()),
    }
}

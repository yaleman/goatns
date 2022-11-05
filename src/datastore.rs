use std::str::from_utf8;

use crate::config::ConfigFile;
use crate::db;
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
        name: String,
        resp: Responder<Option<FileZone>>,
    },
    GetZoneNames {
        resp: Responder<Vec<FileZone>>,
    },
    ImportFile {
        filename: String,
        zone_name: Option<String>,
        resp: Responder<()>,
    },
    Shutdown,
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
    let db_name = from_utf8(&name)
        .map_err(|e| format!("Failed to convert name to utf8 - {e:?}"))
        .unwrap();

    let mut zr = ZoneRecord {
        name: name.clone(),
        typerecords: vec![],
    };

    match db::get_records(conn, db_name.to_string(), rrtype, rclass).await {
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

async fn handle_import_file(
    pool: &Pool<Sqlite>,
    filename: String,
    zone_name: Option<String>,
) -> Result<(), String> {
    let mut txn = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;

    let zones: Vec<FileZone> = crate::zones::load_zones(filename.as_str())?;

    let zones = match zone_name {
        Some(name) => zones.into_iter().filter(|z| z.name == name).collect(),
        None => zones,
    };

    if zones.is_empty() {
        log::warn!("No zones to import!");
        return Err("No zones to import!".to_string());
    }

    for zone in zones {
        use crate::db::DBEntity;

        zone.save_with_txn(&mut txn)
            .await
            .map_err(|e| format!("Failed to load zone {}: {e:?}", zone.name))?;
        log::info!("Imported {}", zone.name);
    }
    txn.commit()
        .await
        .map_err(|e| format!("Failed to start transaction: {e:?}"))?;
    Ok(())
}

async fn handle_get_zone(
    tx: oneshot::Sender<Option<FileZone>>,
    pool: &Pool<Sqlite>,
    name: String,
) -> Result<(), String> {
    let mut txn = pool.begin().await.map_err(|e| format!("{e:?}"))?;

    let zone = crate::db::get_zone_with_txn(&mut txn, &name)
        .await
        .map_err(|e| format!("{e:?}"))?;

    tx.send(zone).map_err(|e| format!("{e:?}"))
}

async fn handle_get_zone_names(
    tx: oneshot::Sender<Vec<FileZone>>,
    pool: &Pool<Sqlite>,
) -> Result<(), String> {
    let mut txn = pool.begin().await.map_err(|e| format!("{e:?}"))?;

    let zones = crate::db::get_zones_with_txn(&mut txn, 100, 0)
        .await
        .map_err(|e| format!("{e:?}"))?;

    tx.send(zones).map_err(|e| format!("{e:?}"))
}

/// Manages the datastore, waits for signals from the server instances and responds with data
pub async fn manager(
    mut rx: mpsc::Receiver<crate::datastore::Command>,
    config: ConfigFile,
) -> Result<(), String> {
    // if they specified a static zone file in the config, then load it
    // let zones = match config.zone_file {
    //     Some(_) => match load_zones_to_tree(&config) {
    //         Ok(value) => value,
    //         Err(error) => {
    //             error!("{}", error);
    //             empty_zones()
    //         }
    //     },
    //     None => empty_zones(),
    // };

    let connpool = match db::get_conn(&config).await {
        Ok(value) => value,
        Err(err) => {
            log::error!("{err}");
            return Err(err);
        }
    };

    // start up the DB
    if let Err(err) = db::start_db(&connpool).await {
        log::error!("{err}");
        return Err(format!("Failed to start DB: {err:?}"));
    };

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::GetZone { name, resp } => {
                let res = handle_get_zone(resp, &connpool, name).await;
                if let Err(e) = res {
                    log::error!("{e:?}")
                };
            }
            Command::GetZoneNames { resp } => {
                let res = handle_get_zone_names(resp, &connpool).await;
                if let Err(e) = res {
                    log::error!("{e:?}")
                };
            }
            Command::Shutdown => {
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
        }
    }

    Ok(())
}

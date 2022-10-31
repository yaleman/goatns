use std::str::from_utf8;

use crate::config::ConfigFile;
use crate::db;
use crate::enums::{RecordClass, RecordType};
use crate::resourcerecord::InternalResourceRecord;
use crate::zones::{empty_zones, load_zones, ZoneRecord};
use log::{debug, error};
use sqlx::{Pool, Sqlite};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
pub enum Command {
    Get {
        /// Reversed vec of the name
        name: Vec<u8>,
        rtype: RecordType,
        rclass: RecordClass,
        resp: Responder<Option<ZoneRecord>>,
    },
    // TODO: create a setter when we're ready to accept updates
    // Set {
    //     name: Vec<u8>,
    //     rtype: RecordType,
    // }
}

async fn handle_get_command(
    // database pool
    conn: &Pool<Sqlite>,
    // this is the result from the things in memory
    zone_get: Option<&ZoneRecord>,
    name: Vec<u8>,
    rtype: RecordType,
    rclass: RecordClass,
    resp: oneshot::Sender<Option<ZoneRecord>>,
) -> Result<(), String> {
    debug!(
        "query name={:?} rtype={rtype:?} rclass={rclass}",
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

    match db::get_records(conn, db_name.to_string(), rtype, rclass).await {
        Ok(value) => zr.typerecords.extend(value),
        Err(err) => {
            log::error!("Failed to query db: {err:?}")
        }
    };

    if let Some(value) = zone_get {
        // check if the type we want is in there, and only return the matching records
        let res: Vec<InternalResourceRecord> = value
            .to_owned()
            .typerecords
            .into_iter()
            .filter(|r| r == &rtype && r == &rclass)
            .collect();
        zr.typerecords.extend(res);
    };

    let result = match zr.typerecords.is_empty() {
        true => None,
        false => Some(zr),
    };

    if let Err(error) = resp.send(result) {
        debug!("error sending response from data store: {:?}", error)
    };
    Ok(())
}

/// Manages the datastore, waits for signals from the server instances and responds with data
pub async fn manager(
    mut rx: mpsc::Receiver<crate::datastore::Command>,
    config: ConfigFile,
) -> Result<(), String> {
    let zones = match load_zones(&config) {
        Ok(value) => value,
        Err(error) => {
            error!("{}", error);
            empty_zones()
        }
    };

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
            Command::Get {
                name,
                rtype,
                rclass,
                resp,
            } => {
                let res = handle_get_command(
                    &connpool,
                    zones.get(name.to_ascii_lowercase()),
                    name,
                    rtype,
                    rclass,
                    resp,
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

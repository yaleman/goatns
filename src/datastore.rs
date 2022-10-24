use std::str::from_utf8;

use crate::config::ConfigFile;
use crate::enums::RecordType;
use crate::resourcerecord::InternalResourceRecord;
use crate::zones::{empty_zones, load_zones, ZoneRecord};
use log::{debug, error};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
pub enum Command {
    Get {
        /// Reversed vec of the name
        name: Vec<u8>,
        rtype: RecordType,
        resp: Responder<Option<ZoneRecord>>,
    },
    // TODO: create a setter when we're ready to accept updates
    // Set {
    //     name: Vec<u8>,
    //     rtype: RecordType,
    // }
}

fn handle_get_command(
    zone_get: Option<&ZoneRecord>,
    name: Vec<u8>,
    rtype: RecordType,
    resp: oneshot::Sender<Option<ZoneRecord>>,
) {
    debug!(
        "searching for name={:?} rtype={:?}",
        from_utf8(&name).unwrap_or("-"),
        rtype
    );

    let result: Option<ZoneRecord> = match zone_get.cloned() {
        Some(value) => {
            // check if the type we want is in there, and only return the matching records
            let res: Vec<InternalResourceRecord> = value
                .to_owned()
                .typerecords
                .into_iter()
                .filter(|r| r == &rtype)
                .collect();
            if res.is_empty() {
                None
            } else {
                let mut zr = value;
                zr.typerecords = res;
                Some(zr)
            }
        }
        None => None,
    };

    if let Err(error) = resp.send(result) {
        debug!("error sending response from data store: {:?}", error)
    };
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

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Get { name, rtype, resp } => {
                handle_get_command(zones.get(name.to_ascii_lowercase()), name, rtype, resp);
            }
        }
    }

    Ok(())
}

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
            // TODO: at some point we should be checking that if the zonerecord has a TTL of None, then it should be pulling from the SOA
            Command::Get { name, rtype, resp } => {
                debug!(
                    "searching for name={:?} rtype={:?}",
                    from_utf8(&name).unwrap_or("-"),
                    rtype
                );

                let result: Option<ZoneRecord> = match zones.get(name.to_ascii_lowercase()).cloned()
                {
                    Some(value) => {
                        let mut zr = value.clone();
                        // check if the type we want is in there, and only return the matching records
                        let res: Vec<InternalResourceRecord> = value
                            .typerecords
                            .into_iter()
                            .filter(|r| r == &rtype)
                            .collect();
                        if res.is_empty() {
                            None
                        } else {
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
        }
    }

    Ok(())
}

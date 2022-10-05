use std::str::from_utf8;

use crate::enums::RecordType;
use crate::zones::{empty_zones, load_zones, ZoneRecord, ZoneRecordType};
use log::{debug, error};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

type Responder<T> = oneshot::Sender<T>;

#[allow(dead_code)]
#[derive(Debug)]
pub enum Command {
    Get {
        /// Reversed vec of the name
        name: Vec<u8>,
        rtype: RecordType,
        resp: Responder<Option<ZoneRecord>>,
    },
    // Set {
    //     name: Vec<u8>,
    //     rtype: RecordType,
    // }
}

// let mut rx: mpsc::Receiver<crate::datastore::Command>;
#[allow(dead_code)]
pub async fn manager(mut rx: mpsc::Receiver<crate::datastore::Command>) -> Result<(), String> {
    let zones = match load_zones() {
        Ok(value) => value,
        Err(error) => {
            error!("{}", error);
            empty_zones()
        }
    };

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Get { name, rtype, resp } => {
                debug!(
                    "searching for name={:?} rtype={:?}",
                    from_utf8(&name).unwrap(),
                    rtype
                );

                // Turn the &ZoneRecord into a ZoneRecord
                let result: Option<ZoneRecord> = match zones.get(name).cloned() {
                    Some(value) => {
                        let mut zr = value.clone();
                        // check if the type we want is in there, and only return the matching records
                        let res: Vec<ZoneRecordType> = value
                            .typerecords
                            .into_iter()
                            .filter(|r| r.rrtype == rtype)
                            .collect();
                        if res.is_empty() {
                            None
                        } else {
                            zr.typerecords = res.to_owned();
                            Some(zr)
                        }
                    }
                    None => None,
                };

                match resp.send(result) {
                    Ok(_) => debug!("sent response"),
                    Err(error) => debug!("error sending response: {:?}", error),
                };
            }
        }
    }

    Ok(())
}

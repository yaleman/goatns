/// zone info
///
///
use log::{debug, error};
use serde::{Deserialize, Serialize};

// use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use crate::enums::RecordType;

/// A DNS Zone in a JSON file
///
#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(dead_code)]
#[serde(rename(serialize = "UPPERCASE"))]
pub struct FileZone {
    /// MNAME The <domain-name> of the name server that was the original or primary source of data for this zone.
    #[serde(rename(serialize = "MNAME"))]
    pub name: String,

    // RNAME A <domain-name> which specifies the mailbox of the person responsible for this zone.
    #[serde(rename(serialize = "RNAME"), default = "rname_default")]
    pub rname: String,
    /// REFRESH - A 32 bit time interval before the zone should be refreshed.
    #[serde(default)]
    pub refresh: u32,
    /// RETRY - A 32 bit time interval that should elapse before a failed refresh should be retried.
    #[serde(default)]
    pub retry: u32,
    /// SERIAL - The unsigned 32 bit version number of the original copy of the zone.  Zone transfers preserve this value.  This value wraps and should be compared using sequence space arithmetic.
    #[serde(default)]
    pub serial: u32,
    /// MINIMUM - The unsigned 32 bit minimum TTL field that should be exported with any RR from this zone.
    #[serde(default)]
    pub minimum: u32,
    ///  EXPIRE - A 32 bit time value that specifies the upper limit on the time interval that can elapse before the zone is no longer authoritative.
    #[serde(default)]
    pub expire: u32,
    pub records: Vec<FileZoneRecord>,
}

/// default RNAME value for FileZone
pub fn rname_default() -> String {
    String::from("barry.goat")
}

/// A DNS Record from the JSON file
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct FileZoneRecord {
    name: String,
    rrtype: String,
    #[serde(with = "serde_bytes")]
    rdata: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ZoneRecord {
    name: Vec<u8>,
    rrtype: RecordType,
    rdata: Vec<Vec<u8>>,
}

#[cfg(test)]
mod test {
    #[test]
    fn test_foo() {
        assert_eq!(1, 1);
    }
}

pub fn load_zones() -> Result<Vec<FileZone>, String> {
    let mut file = match File::open("testzones.json") {
        Ok(value) => value,
        Err(error) => {
            let emsg = format!("Failed to open file: {:?}", error);
            error!("{}", emsg);
            return Err(emsg);
        }
    };

    let mut buf: String = String::new();
    file.read_to_string(&mut buf).unwrap();
    let jsonstruct: Vec<FileZone> = match json5::from_str(&buf) {
        Ok(value) => value,
        Err(error) => {
            let emsg = format!("Failed to read JSON file: {:?}", error);
            error!("{}", emsg);
            return Err(emsg);
        }
    };
    debug!("{:?}", jsonstruct);

    let response = vec![];
    Ok(response)
}

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
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct FileZone {
    name: String,
    pub records: Vec<FileZoneRecord>,
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

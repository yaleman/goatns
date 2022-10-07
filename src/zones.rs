// use crate::enums::RecordType;
use crate::resourcerecord::InternalResourceRecord;
// use crate::{rdata};
/// zone info
///
///
use log::{debug, error};
use patricia_tree::PatriciaMap;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::str::from_utf8;

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
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[allow(dead_code)]
pub struct FileZoneRecord {
    pub name: String,
    pub rrtype: String,
    #[serde(with = "serde_bytes")]
    pub rdata: Vec<u8>,
    pub ttl: Option<u32>,
}

// #[derive(Debug, PartialEq, Eq, Clone)]
// pub struct ZoneRecordType {
//     pub rrtype: RecordType,
//     pub rdata: Vec<Vec<u8>>,
// }

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ZoneRecord {
    // the full name including the zone
    pub name: Vec<u8>,
    pub typerecords: Vec<InternalResourceRecord>,
}

impl Display for ZoneRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Name: {:?} Name Bytes: {:?} Records: {:?}",
            from_utf8(&self.name),
            &self.name,
            self.typerecords
        ))
    }
}

// impl From<FileZoneRecord> for ZoneRecord {
//     fn from(fzr: FileZoneRecord) -> Self {
//         ZoneRecord {
//             name: fzr.name.as_bytes().to_vec(),
//             typerecords: vec![ZoneRecordType {
//                 rrtype: fzr.rrtype.as_str().into(),
//                 rdata: vec![fzr.rdata],
//             }],
//         }
//     }
// }

#[cfg(test)]
mod test {
    #[test]
    fn test_foo() {
        assert_eq!(1, 1);
    }
}

pub fn empty_zones() -> PatriciaMap<ZoneRecord> {
    let tree: PatriciaMap<ZoneRecord> = PatriciaMap::new();
    tree
}

/// Load the data from a JSON file on disk
pub fn load_zones() -> Result<PatriciaMap<ZoneRecord>, String> {
    let zone_filename = "zones.json";
    let mut file = match File::open(zone_filename) {
        Ok(value) => value,
        Err(error) => {
            return Err(format!("Failed to open zone file: {:?}", error));
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

    let mut tree = empty_zones();
    for zone in jsonstruct {
        for record in zone.records {
            eprintln!("fzr: {:?}", record);
            let record_data: InternalResourceRecord = record.clone().into();

            // mush the record name and the zone name together

            let mut name: Vec<u8>;
            let record_name = record.clone().name;
            if record_name == *"@" {
                name = zone.name.as_bytes().to_vec()
            } else {
                name = format!("{}.{}", record.clone().name, zone.name)
                    .as_bytes()
                    .to_vec()
            };
            // I spin you right round baby, right round...
            name.reverse();

            if tree.contains_key(&name) {
                let existing_value = tree.get_mut(&name).unwrap();
                existing_value.typerecords.push(record_data);
                let toinsert = existing_value.clone();
                tree.insert(name, toinsert);
            } else {
                tree.insert(
                    &name,
                    ZoneRecord {
                        name: name.clone(),
                        typerecords: vec![record_data],
                    },
                );
            }
        }
    }
    Ok(tree)
}

#[allow(dead_code)]
struct ZoneTreeLeaf {
    wildcards: Vec<ZoneRecord>,
}

use crate::enums::RecordType;
use crate::rdata;
/// zone info
///
///
use log::{debug, error};
use patricia_tree::PatriciaMap;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

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
    name: String,
    rrtype: String,
    #[serde(with = "serde_bytes")]
    rdata: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ZoneRecordType {
    pub rrtype: RecordType,
    pub rdata: Vec<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ZoneRecord {
    // the full name including the zone
    pub name: Vec<u8>,
    pub typerecords: Vec<ZoneRecordType>,
}

impl From<FileZoneRecord> for ZoneRecord {
    fn from(fzr: FileZoneRecord) -> Self {
        ZoneRecord {
            name: fzr.name.as_bytes().to_vec(),
            typerecords: vec![ZoneRecordType {
                rrtype: fzr.rrtype.as_str().into(),
                rdata: vec![fzr.rdata],
            }],
        }
    }
}

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
            let rrtype: RecordType = record.rrtype.as_str().into();
            // handle the various types and put them into the thing nicer
            let rdata: Vec<u8> = match rrtype {
                RecordType::A => rdata::RdataA::from(record.rdata).address.to_vec(),
                RecordType::NS => todo!(),
                RecordType::MD => todo!(),
                RecordType::MF => todo!(),
                RecordType::CNAME => todo!(),
                RecordType::SOA => todo!(),
                RecordType::MB => todo!(),
                RecordType::MG => todo!(),
                RecordType::MR => todo!(),
                RecordType::NULL => todo!(),
                RecordType::WKS => todo!(),
                RecordType::PTR => todo!(),
                RecordType::HINFO => todo!(),
                RecordType::MINFO => todo!(),
                RecordType::MX => todo!(),
                RecordType::TXT => record.rdata,
                RecordType::AAAA => rdata::RdataAAAA::from(record.rdata).rdata.to_vec(),
                RecordType::AXFR => todo!(),
                RecordType::MAILB => todo!(),
                RecordType::MAILA => todo!(),
                RecordType::ALL => todo!(),
                // if this comes back, woo!
                RecordType::InvalidType => vec![],
            };

            // mush the record name and the zone name together

            let mut name: Vec<u8>;
            if record.name == *"@" {
                name = zone.name.as_bytes().to_vec()
            } else {
                name = format!("{}.{}", record.name, zone.name).as_bytes().to_vec()
            };
            // I spin you right round baby, right round...
            name.reverse();

            let zonerecordtype = ZoneRecordType {
                rrtype: record.rrtype.as_str().into(),
                rdata: vec![rdata],
            };

            if tree.contains_key(&name) {
                let existing_value = tree.get_mut(&name).unwrap();
                existing_value.typerecords.push(zonerecordtype);
                let toinsert = existing_value.clone();
                tree.insert(name, toinsert);
            } else {
                tree.insert(
                    &name,
                    ZoneRecord {
                        name: name.clone(),
                        typerecords: vec![zonerecordtype],
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

use crate::config::ConfigFile;
use crate::enums::{RecordClass, RecordType};
use crate::resourcerecord::{DomainName, InternalResourceRecord};
use log::{debug, error, info};
use patricia_tree::PatriciaMap;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::from_utf8;

/// A DNS Zone in a JSON file
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename(serialize = "UPPERCASE"))]
pub struct FileZone {
    #[serde(default = "default_id")]
    pub id: u64,
    /// MNAME The <domain-name> of the name server that was the original or primary source of data for this zone.
    #[serde(rename(serialize = "MNAME"))]
    pub name: String,
    // RNAME A <domain-name> which specifies the mailbox of the person responsible for this zone.
    #[serde(rename(serialize = "RNAME"), default = "rname_default")]
    pub rname: String,
    /// SERIAL - The unsigned 32 bit version number of the original copy of the zone.  Zone transfers preserve this value.  This value wraps and should be compared using sequence space arithmetic.
    #[serde(default)]
    pub serial: u32,
    /// REFRESH - A 32 bit time interval before the zone should be refreshed.
    #[serde(default)]
    pub refresh: u32,
    /// RETRY - A 32 bit time interval that should elapse before a failed refresh should be retried.
    #[serde(default)]
    pub retry: u32,
    ///  EXPIRE - A 32 bit time value that specifies the upper limit on the time interval that can elapse before the zone is no longer authoritative.
    #[serde(default)]
    pub expire: u32,
    /// MINIMUM - The unsigned 32 bit minimum TTL field that should be exported with any RR from this zone.
    #[serde(default)]
    pub minimum: u32,
    pub records: Vec<FileZoneRecord>,
}

impl FileZone {
    /// Checks if they're equal, ignores the zone id and records
    pub fn matching_data(&self, cmp: &FileZone) -> bool {
        self.expire == cmp.expire
            && self.minimum == cmp.minimum
            && self.name == cmp.name
            && self.refresh == cmp.refresh
            && self.retry == cmp.retry
            && self.rname == cmp.rname
            && self.serial == cmp.serial
    }
}
/// default RNAME value for FileZone
pub fn rname_default() -> String {
    String::from("barry.goat")
}

/// A DNS Record from the JSON file
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct FileZoneRecord {
    #[serde(default = "default_id")]
    pub zoneid: u64,
    #[serde(default)]
    pub id: u64,
    #[serde(default = "default_record_name")]
    pub name: String,
    pub rrtype: String,
    #[serde(default = "default_record_class")]
    pub class: RecordClass,
    pub rdata: String,
    pub ttl: u32,
}

/// If you don't specify a name, it's the root.
fn default_id() -> u64 {
    1
}
/// If you don't specify a name, it's the root.
fn default_record_name() -> String {
    String::from("@")
}
/// Sets a default of IN because well, what else would you use?
fn default_record_class() -> RecordClass {
    RecordClass::Internet
}

impl Display for FileZoneRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "FileZoneRecord {{ name={} class={} rrtype={}, ttl={}, zoneid={}, id={}, rdata={} }}",
            self.name, self.class, self.rrtype, self.ttl, self.zoneid, self.id, self.rdata
        ))
    }
}

impl TryFrom<SqliteRow> for FileZoneRecord {
    type Error = String;
    fn try_from(row: SqliteRow) -> Result<Self, String> {
        let zoneid: i64 = row.get("zoneid");
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        let rrtype: u16 = row.get("rrtype");
        let class: u16 = row.get("rclass");
        let rdata: String = row.get("rdata");
        let ttl: u32 = row.get("ttl");

        Ok(FileZoneRecord {
            zoneid: zoneid as u64,
            id: id as u64,
            name,
            rrtype: RecordType::from(&rrtype).to_string(),
            class: RecordClass::from(&class),
            rdata,
            ttl,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
/// This is used when storing a set of records in the memory-based datastore
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
            self.typerecords,
        ))
    }
}

pub fn empty_zones() -> PatriciaMap<ZoneRecord> {
    let tree: PatriciaMap<ZoneRecord> = PatriciaMap::new();
    tree
}

pub fn load_zone_from_file(filename: &Path) -> Result<FileZone, String> {
    let mut file = match File::open(filename) {
        Ok(value) => value,
        Err(error) => {
            return Err(format!("Failed to open zone file: {:?}", error));
        }
    };
    let mut buf: String = String::new();
    file.read_to_string(&mut buf).unwrap();
    let jsonstruct: FileZone = match json5::from_str(&buf) {
        Ok(value) => value,
        Err(error) => {
            let emsg = format!("Failed to read JSON file: {:?}", error);
            error!("{}", emsg);
            return Err(emsg);
        }
    };
    Ok(jsonstruct)
}

/// Load the data from a JSON file on disk
pub fn load_zones(config: &ConfigFile) -> Result<PatriciaMap<ZoneRecord>, String> {
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
    if config.enable_hinfo {
        info!("Enabling HINFO response on hinfo.goat");
        let hinfo_name = String::from("hinfo.goat");
        tree.insert(
            hinfo_name.clone(),
            ZoneRecord {
                name: hinfo_name.into_bytes(),
                typerecords: vec![InternalResourceRecord::HINFO {
                    cpu: None,
                    os: None,
                    ttl: 1,
                    rclass: crate::RecordClass::Chaos,
                }],
            },
        );
    };

    for zone in jsonstruct {
        // here we add the SOA
        let soa = InternalResourceRecord::SOA {
            mname: DomainName::from("server.lol"), // TODO: get our hostname, or configure it in the config
            zone: DomainName::from(zone.name.as_str()),
            rname: DomainName::from(zone.rname.as_str()),
            serial: zone.serial,
            refresh: zone.refresh,
            retry: zone.retry,
            expire: zone.expire,
            minimum: zone.minimum,

            rclass: crate::RecordClass::Internet,
        };
        debug!("{soa:?}");
        tree.insert(
            &zone.name,
            ZoneRecord {
                name: zone.name.as_bytes().to_vec(),
                typerecords: vec![soa],
            },
        );

        for record in zone.records {
            debug!("fzr: {:?}", record);

            // mush the record name and the zone name together
            let name = match record.name.as_str() {
                "@" => zone.name.clone(),
                _ => {
                    let res = format!("{}.{}", record.clone().name, zone.name);
                    res
                }
            };

            let record_data: InternalResourceRecord = match record.try_into() {
                Ok(value) => value,
                Err(error) => {
                    error!("Error loading record: {error:?}");
                    continue;
                }
            };

            if tree.contains_key(&name) {
                let existing_value = tree.get_mut(&name).unwrap();
                existing_value.typerecords.push(record_data);
                let toinsert = existing_value.clone();
                tree.insert(name, toinsert);
            } else {
                tree.insert(
                    &name,
                    ZoneRecord {
                        name: name.clone().into_bytes(),
                        typerecords: vec![record_data],
                    },
                );
            }
        }
    }
    Ok(tree)
}

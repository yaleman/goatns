use crate::enums::RecordClass;
use crate::resourcerecord::InternalResourceRecord;
use log::*;

use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::from_utf8;

/// A DNS Zone
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename(serialize = "UPPERCASE"))]
pub struct FileZone {
    /// Database row ID
    pub id: Option<i64>,
    /// MNAME The <domain-name> of the name server that was the original or primary source of data for this zone.
    // #[serde(rename(serialize = "MNAME"))]
    pub name: String,
    // RNAME A <domain-name> which specifies the mailbox of the person responsible for this zone.
    // #[serde(rename(serialize = "RNAME"), default = "rname_default")]
    #[serde(default = "rname_default")]
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
    String::from("barry.dot.goat")
}

/// A DNS Record from the JSON file
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct FileZoneRecord {
    /// Foreign key to id in [FileZone::id]
    pub zoneid: Option<i64>,
    /// Database row ID
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default = "default_record_name")]
    pub name: String,
    pub rrtype: String,
    #[serde(default = "default_record_class")]
    pub class: RecordClass,
    pub rdata: String,
    pub ttl: u32,
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
            "FileZoneRecord {{ name={} class={} rrtype={}, ttl={}, zoneid={:#?}, id={:#?}, rdata={} }}",
            self.name, self.class, self.rrtype, self.ttl, self.zoneid, self.id, self.rdata
        ))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
/// A list of records associated with a given name - ie `foo.example.com -> [A { 1.2.3.4}, AAAA { 2000:cafe:beef }` etc
pub struct ZoneRecord {
    /// the full name including the zone
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

pub fn load_zones(filename: &str) -> Result<Vec<FileZone>, String> {
    let mut file = match File::open(filename) {
        Ok(value) => value,
        Err(error) => {
            return Err(format!("Failed to open zone file: {:?}", error));
        }
    };

    let mut buf: String = String::new();
    file.read_to_string(&mut buf).unwrap();
    let jsonstruct: Result<Vec<FileZone>, String> =
        json5::from_str(&buf).map_err(|e| format!("Failed to read JSON file: {e:?}"));
    jsonstruct
}

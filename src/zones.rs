use crate::config::ConfigFile;
use crate::enums::RecordClass;
use crate::resourcerecord::{DomainName, InternalResourceRecord};
use log::{debug, error, info};
use patricia_tree::PatriciaMap;

use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::str::from_utf8;

/// A DNS Zone in a JSON file
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename(serialize = "UPPERCASE"))]
pub struct FileZone {
    #[serde(default)]
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

/// default RNAME value for FileZone
pub fn rname_default() -> String {
    String::from("barry.goat")
}

/// A DNS Record from the JSON file
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct FileZoneRecord {
    #[serde(default = "default_record_name")]
    pub name: String,
    pub rrtype: String,
    // #[serde(with = "serde_bytes")]
    pub rdata: String,
    pub ttl: u32,
    #[serde(default = "default_record_class")]
    pub class: RecordClass,
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
            "FileZoneRecord {{ name: {} rrtype: {}, rdata: {}, ttl: {} }}",
            self.name, self.rrtype, self.rdata, self.ttl
        ))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ZoneRecord {
    // the full name including the zone
    pub name: Vec<u8>,
    pub typerecords: Vec<InternalResourceRecord>,
    // pub rname: String,
    // pub serial: u32,
    // pub refresh: u32,
    // pub retry: u32,
    // pub expire: u32,
    // pub minimum_ttl: u32,
}

impl Display for ZoneRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Name: {:?} Name Bytes: {:?} Records: {:?}", //, Rname: {} Serial: {} Refresh: {} Retry: {} Expire: {} Minimum TTL: {}
            from_utf8(&self.name),
            &self.name,
            self.typerecords,
            // self.rname,
            // self.serial,
            // self.refresh,
            // self.retry,
            // self.expire,
            // self.minimum_ttl,
        ))
    }
}

pub fn empty_zones() -> PatriciaMap<ZoneRecord> {
    let tree: PatriciaMap<ZoneRecord> = PatriciaMap::new();
    tree
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

#[allow(dead_code)]
// TODO: this should be the end of the tree, so we can cover wildcards
struct ZoneTreeLeaf {
    wildcards: Vec<ZoneRecord>,
}

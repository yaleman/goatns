use crate::enums::RecordClass;
use crate::error::GoatNsError;
use crate::resourcerecord::InternalResourceRecord;
use crate::web::api::records::ZoneFileRecord;
use crate::{db::entities, web::api::zones::ZoneForm};
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet};
use tracing::*;

use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::from_utf8;
use utoipa::ToSchema;

use sea_orm::{ActiveValue::Set, DatabaseConnection, TransactionTrait};
use uuid::Uuid;

/// A DNS Zone loaded from a file
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct FileZone {
    /// Database row ID (ignored when loading from file, new UUID is generated)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// MNAME The `domain-name` of the name server that was the original or primary source of data for this zone.
    pub name: String,
    /// RNAME A `domain-name` which specifies the mailbox of the person responsible for this zone.
    #[serde(default = "rname_default")]
    pub rname: String,
    /// SERIAL - The unsigned 32 bit version number of the original copy of the zone.
    #[serde(default)]
    pub serial: u32,
    /// REFRESH - A 32 bit time interval before the zone should be refreshed.
    #[serde(default)]
    pub refresh: u32,
    /// RETRY - A 32 bit time interval that should elapse before a failed refresh should be retried.
    #[serde(default)]
    pub retry: u32,
    /// EXPIRE - A 32 bit time value that specifies the upper limit on the time interval that can elapse before the zone is no longer authoritative.
    #[serde(default)]
    pub expire: u32,
    /// MINIMUM - The unsigned 32 bit minimum TTL field that should be exported with any RR from this zone.
    #[serde(default)]
    pub minimum: u32,
    /// The records associated with this zone
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

    #[instrument(level = "debug")]
    pub fn get_soa_record(&self, server_hostname: &str) -> InternalResourceRecord {
        InternalResourceRecord::SOA {
            zone: self.name.clone().into(),
            mname: server_hostname.into(),
            rname: self.rname.clone().into(),
            serial: self.serial,
            refresh: self.refresh,
            retry: self.retry,
            expire: self.expire,
            minimum: self.minimum,
            rclass: RecordClass::Internet,
        }
    }

    /// Save the zone and its records to the database
    #[instrument(level = "debug", skip(db))]
    pub async fn save(
        &self,
        db: &DatabaseConnection,
    ) -> Result<entities::zones::Model, GoatNsError> {
        let txn = db.begin().await?;

        // Create the zone
        let zone = entities::zones::ActiveModel {
            id: NotSet,
            name: Set(self.name.clone()),
            rname: Set(self.rname.clone()),
            serial: Set(self.serial),
            refresh: Set(self.refresh),
            retry: Set(self.retry),
            expire: Set(self.expire),
            minimum: Set(self.minimum),
        };

        let zone_model = zone.insert(&txn).await?;

        // Create all the records
        for record in &self.records {
            let rrtype: crate::enums::RecordType = record.rrtype.as_str().into();

            let record_model = entities::records::ActiveModel {
                id: NotSet,
                zoneid: Set(zone_model.id),
                name: Set(record.name.clone()),
                ttl: Set(Some(record.ttl)),
                rrtype: Set(rrtype as u16),
                rclass: Set(record.class as u16),
                rdata: Set(record.rdata.clone()),
            };
            record_model.insert(&txn).await?;
        }

        txn.commit().await?;
        Ok(zone_model)
    }
}

/// default RNAME value for FileZone
pub fn rname_default() -> String {
    String::from("barry.dot.goat")
}

/// A DNS Record from the JSON file
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, ToSchema)]
pub struct FileZoneRecord {
    /// Database row ID
    #[serde(default)]
    pub id: Option<i64>,
    /// Foreign key to id in [FileZone::id]
    pub zoneid: Option<i64>,
    #[serde(default = "default_record_name")]
    /// The name of the record
    pub name: String,
    /// The type of record
    pub rrtype: String,
    #[serde(default = "default_record_class")]
    /// The class of the record
    pub class: RecordClass,
    /// The actual data for the record
    pub rdata: String,
    /// Time to live
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

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
/// A list of records associated with a given name - ie `foo.example.com -> [A { 1.2.3.4}, AAAA { 2000:cafe:beef }` etc
pub struct ZoneRecord {
    /// the full name including the zone
    pub name: Vec<u8>,
    /// the records associated with this name
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

/// Loads a zone file
pub fn load_zone_from_file(filename: &Path) -> Result<ZoneFile, GoatNsError> {
    let mut file = match File::open(filename) {
        Ok(value) => value,
        Err(err) => {
            return Err(GoatNsError::FileError(format!(
                "Failed to open zone file: {err:?}",
            )));
        }
    };
    let mut buf: String = String::new();
    file.read_to_string(&mut buf)
        .inspect_err(|err| error!("Failed to read {}: {:?}", &filename.display(), err))?;
    let jsonstruct: ZoneFile = match json5::from_str(&buf) {
        Ok(value) => value,
        Err(err) => {
            let emsg = format!("Failed to read JSON file: {err:?}");
            error!("{emsg}");
            return Err(GoatNsError::FileError(emsg));
        }
    };
    Ok(jsonstruct)
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ZoneFile {
    #[serde(flatten)]
    pub zone: ZoneForm,
    pub records: Vec<ZoneFileRecord>,
}

/// Loads a zone file
#[instrument(level = "debug")]
pub fn load_zones(filename: &str) -> Result<Vec<ZoneFile>, GoatNsError> {
    let mut file = match File::open(filename) {
        Ok(value) => value,
        Err(err) => {
            return Err(GoatNsError::FileError(format!(
                "Failed to open zone file: {err:?}",
            )));
        }
    };

    let mut buf: String = String::new();
    file.read_to_string(&mut buf)
        .inspect_err(|err| error!("Failed to read {}: {:?}", filename, err))?;
    let jsonblob: Vec<serde_json::Value> =
        json5::from_str(&buf).map_err(|err| GoatNsError::FileError(err.to_string()))?;
    let mut zones: Vec<ZoneFile> = Vec::new();

    for zone_json in jsonblob.into_iter() {
        zones.push(
            serde_json::from_value(zone_json.clone())
                .inspect_err(|err| error!(error=?err, zone=?zone_json, "Failed to parse zone"))?,
        );
    }

    Ok(zones)
}

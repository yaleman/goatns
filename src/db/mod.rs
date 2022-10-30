use crate::enums::{RecordClass, RecordType};

use crate::resourcerecord::DomainName;
use crate::zones::FileZone;
// use crate::config::get_config;
// use crate::RecordClass;
//
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};

// #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
// pub struct Zone {
//     pub id: u64,
//     // The zone that this SOA record is for - eg hello.goat or example.com
//     pub name: String,
//     /// A <domain-name> which specifies the mailbox of the person responsible for this zone. eg: `dns.example.com` is actually `dns@example.com`
//     pub rname: String,
//     pub serial: u32,
//     pub refresh: u32,
//     pub retry: u32,
//     pub expire: u32,
//     pub minimum: u32,
//     pub re
//     // pub rclass: RecordClass,
// }

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Record {
    // The zone that this SOA record is for - eg hello.goat or example.com
    pub name: DomainName,
    pub ttl: u32,
    pub zoneid: u64,
    pub rtype: u16,
    pub rclass: u16,
    pub rdata: Vec<u8>,
}

#[allow(dead_code)]
pub fn create_zones_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    // let config = get_config(None);

    log::trace!("Creating Zones Table");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS
        zones (
            id   INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            rname TEXT NOT NULL,
            serial INTEGER NOT NULL,
            refresh INTEGER NOT NULL,
            retry INTEGER NOT NULL,
            expire INTEGER NOT NULL,
            minimum INTEGER NOT NULL
        )",
        (), // empty list of parameters.
    )?;
    log::trace!("Creating Zones Index");
    conn.execute(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_zones
        ON zones (
            id,name
        )",
        (), // empty list of parameters.
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn create_records_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    log::trace!("Creating Records Table");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS
        records (
            id      INTEGER PRIMARY KEY,
            zoneid  INTEGER NOT NULL,
            name    TEXT, /* this can be null for apex records */
            ttl     INTEGER,
            rtype   INTEGER NOT NULL,
            rclass  INTEGER NOT NULL,
            rdata   TEXT NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id)
        )",
        (), // empty list of parameters.
    )?;
    log::trace!("Creating Records Index");
    conn.execute(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_records
        ON records (
            id,zoneid,name,rtype,rclass
        )",
        (), // empty list of parameters.
    )?;
    log::trace!("Creating Records view");
    // this view lets us query based on the full name
    conn.execute(
        "CREATE VIEW record_merged ( record_id, zone_id, rtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zone_id, records.rtype, records.rclass ,records.rdata,
        CASE
            WHEN records.name is NULL THEN zones.name
            ELSE records.name || '.' || zones.name
        END AS name,
        CASE WHEN records.ttl is NULL then zones.minimum
            WHEN records.ttl > zones.minimum THEN records.ttl
            ELSE records.ttl
        END AS ttl
        from records, zones where records.zoneid = zones.id",
        (), // empty list of parameters.
    )?;
    Ok(())
}

#[allow(dead_code)]
/// define a zone
pub fn create_zone(conn: &Connection, zone: FileZone) -> Result<usize, rusqlite::Error> {
    let result = conn.execute(
        "INSERT INTO zones (name, rname, serial, refresh, retry, expire, minimum)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (
            &zone.name,
            &zone.rname,
            &zone.serial,
            &zone.refresh,
            &zone.retry,
            &zone.expire,
            &zone.minimum,
        ),
    )?;

    Ok(result)
}

#[allow(dead_code)]
/// create a resource record within a zone
pub fn create_record(conn: &Connection, record: Record) -> Result<usize, rusqlite::Error> {
    let result = conn.execute(
        "INSERT INTO records (zoneid, name, ttl, rtype, rclass, rdata)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (
            &record.zoneid,
            &record.name.name,
            &record.ttl,
            &record.rtype,
            &record.rclass,
            &record.rdata,
        ),
    )?;
    Ok(result)
}

#[allow(dead_code)]
pub fn get_zone(conn: &Connection, name: String) -> Result<Option<FileZone>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT
        id, name, rname, serial, refresh, retry, expire, minimum
        FROM zones
        WHERE name = ?",
    )?;
    let mut res = stmt.query([name])?;

    let row = match res.next()? {
        None => return Ok(None),
        Some(value) => value,
    };

    let id: u64 = row.get(0)?;

    Ok(Some(FileZone {
        id,
        name: row.get(1)?,
        rname: row.get(2)?,
        serial: row.get(3)?,
        refresh: row.get(4)?,
        retry: row.get(5)?,
        expire: row.get(6)?,
        minimum: row.get(7)?,
        records: vec![],
    }))
}

#[allow(dead_code)]
pub fn get_record(
    conn: &Connection,
    name: String,
    rtype: RecordType,
    rclass: RecordClass,
) -> Result<Option<Record>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT
        record_id, zone_id, name, rclass, rtype, rdata
        FROM record_merged
        WHERE name = ? AND rtype = ? AND rclass = ?",
    )?;
    let search_rtype = (rtype as u16).to_string();
    let search_rclass = (rclass as u16).to_string();

    let mut res = stmt.query([name, search_rtype, search_rclass])?;

    let row = match res.next()? {
        None => return Ok(None),
        Some(value) => value,
    };

    let id: u64 = row.get(0)?;
    let zoneid: u64 = row.get(1)?;
    print!("Got row id={id}");
    let name: String = row.get(2)?;
    let rclass: u16 = row.get(3)?;
    let rtype: u16 = row.get(3)?;
    let rdata: Vec<u8> = row.get(3)?;
    // let rname: String = row.get(2)?;

    Ok(Some(Record {
        name: DomainName::from(name),
        ttl: 1,
        zoneid,
        rtype,
        rclass,
        rdata,
    }))
}

#[test]
fn test_get_zone_empty() -> Result<(), rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    println!("Creating Zones Table");
    create_zones_table(&conn)?;

    let zone_data = get_zone(&conn, "example.org".to_string())?;
    println!("{:?}", zone_data);
    assert_eq!(zone_data, None);
    Ok(())
}

#[test]
fn test_db_create_table_zones() -> Result<(), rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    Ok(create_zones_table(&conn)?)
}

#[test]
fn test_db_create_table_records() -> Result<(), rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    println!("Creating Records Table");
    Ok(create_records_table(&conn)?)
}

#[cfg(test)]
pub fn test_example_com_zone() -> FileZone {
    FileZone {
        id: 1,
        name: String::from("example.com"),
        rname: String::from("billy.example.com"),
        ..FileZone::default()
    }
}

#[test]
fn test_db_create_records() -> Result<(), rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    println!("Creating Zones Table");
    create_zones_table(&conn)?;
    println!("Creating Records Table");
    create_records_table(&conn)?;

    println!("Creating Zone");
    create_zone(&conn, test_example_com_zone())?;

    println!("Creating Record");
    create_record(
        &conn,
        Record {
            name: DomainName::from("foo"),
            ttl: 123,
            zoneid: 1,
            rtype: RecordType::TXT as u16,
            rclass: RecordClass::Internet as u16,
            rdata: "test txt".as_bytes().to_vec(),
        },
    )?;

    let res = get_record(
        &conn,
        "foo".to_string(),
        RecordType::TXT,
        RecordClass::Internet,
    )?;
    println!("Record: {res:?}");
    Ok(())
}

#[test]
fn test_all_db_things() -> Result<(), rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    // let conn = Connection::open("./goatns.sqlite3")?;
    println!("Creating Zones Table");
    create_zones_table(&conn)?;
    println!("Creating Records Table");
    create_records_table(&conn)?;
    println!("Successfully created tables!");

    let zone = test_example_com_zone();

    println!("Creating a zone");
    create_zone(&conn, zone.clone())?;
    println!("Getting a zone!");
    let zone_data = get_zone(&conn, "example.com".to_string())?;
    println!("{:?}", zone_data);
    assert_eq!(zone_data, Some(zone));
    let zone_data = get_zone(&conn, "example.org".to_string())?;
    println!("{:?}", zone_data);
    assert_eq!(zone_data, None);

    create_record(
        &conn,
        Record {
            name: DomainName::from("foo"),
            ttl: 123,
            zoneid: 1,
            rtype: RecordType::TXT as u16,
            rclass: RecordClass::Internet as u16,
            rdata: "test txt".as_bytes().to_vec(),
        },
    )?;

    Ok(())
}

/*

drop view row_coalesce;



select record_id, name from row_coalesce;
 */

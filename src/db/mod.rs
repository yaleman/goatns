use std::str::FromStr;
use std::time::Duration;

use crate::config::ConfigFile;
use crate::enums::{RecordClass, RecordType};

use crate::resourcerecord::InternalResourceRecord;
use crate::zones::{FileZone, FileZoneRecord};
use async_trait::async_trait;

use serde::{Deserialize, Serialize};
use sqlx::pool::PoolConnection;
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions, SqliteRow};
use sqlx::{Arguments, ConnectOptions, Connection, Pool, Row, Sqlite, SqlitePool, Transaction};

#[cfg(test)]
mod test;

const SQL_VIEW_RECORDS: &str = "records_merged";

pub async fn get_conn(config: &ConfigFile) -> Result<Pool<Sqlite>, String> {
    let db_path: &str = &shellexpand::full(&config.sqlite_path).unwrap();
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    log::debug!("Opening Database: {db_url}");

    let mut options = match SqliteConnectOptions::from_str(&db_url) {
        Ok(value) => value,
        Err(error) => return Err(format!("connection failed: {error:?}")),
    };
    options.log_statements(log::LevelFilter::Trace);
    // log anything that takes longer than 1s
    // TODO: make this configurable
    options.log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(1));

    match SqlitePool::connect_with(options).await {
        Ok(value) => Ok(value),
        Err(err) => Err(format!("Error opening SQLite DB ({db_url:?}): {err:?}")),
    }
}

/// Do the basic setup and checks (if we write any)
pub async fn start_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    create_zones_table(pool).await?;
    create_users_table(pool).await?;
    create_records_table(pool).await?;
    create_ownership_table(pool).await?;
    log::info!("Completed DB Startup!");
    Ok(())
}

pub async fn create_users_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    log::debug!("Ensuring DB Users table exists");
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS
        users (
            id  INTEGER PRIMARY KEY,
            username TEXT NOT NULL,
            email TEXT NOT NULL,
            disabled BOOL NOT NULL
        )"#,
    )
    .execute(&mut pool.acquire().await?)
    .await?;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS
        ind_users_fields
        ON users ( username, email )",
    )
    .execute(&mut pool.acquire().await?)
    .await?;

    Ok(())
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct User {
    #[serde(default)]
    pub id: u64,
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub owned_zones: Vec<u64>,
}

impl User {
    #[allow(dead_code, unused_variables)]
    pub async fn create(&self, pool: &SqlitePool) -> Result<usize, sqlx::Error> {
        // TODO: test user create
        let res = sqlx::query("INSERT into users (username, email, disabled) VALUES(?, ?, ?)")
            .bind(&self.username)
            .bind(&self.email)
            .bind(false)
            .execute(&mut pool.acquire().await?)
            .await?;

        Ok(1)
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // TODO: test user delete
        let mut txn = pool.begin().await?;

        let res = sqlx::query("DELETE FROM ownership WHERE userid = ?")
            .bind(self.id as i64)
            .execute(&mut *txn)
            .await?;

        let res = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(self.id as i64)
            .execute(&mut *txn)
            .await?;

        txn.commit().await?;

        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ZoneOwnership {
    #[serde(default)]
    id: u64,
    pub userid: u64,
    pub zoneid: u64,
}

impl ZoneOwnership {
    #[allow(dead_code, unused_variables)]
    pub async fn create(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // TODO: test ownership create
        sqlx::query(
            "INSERT INTO ownership (zoneid, userid) VALUES ( ?, ? ) ON CONFLICT DO NOTHING",
        )
        .bind(self.zoneid as i64)
        .bind(self.userid as i64)
        .execute(&mut pool.acquire().await?)
        .await?;
        Ok(())
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete(&self, pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        // TODO: test ownership delete
        let res = sqlx::query("DELETE FROM ownership WHERE zoneid = ? AND userid = ?")
            .bind(self.zoneid as i64)
            .bind(self.userid as i64)
            .execute(&mut pool.acquire().await?)
            .await?;
        Ok(res.rows_affected())
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete_for_user(self, pool: &SqlitePool) -> Result<User, sqlx::Error> {
        // TODO: test user delete
        // TODO: delete all ownership records
        todo!();
    }
}

pub async fn create_ownership_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await.unwrap();

    #[cfg(test)]
    eprintln!("Ensuring DB Ownership table exists");
    log::debug!("Ensuring DB Ownership table exists");
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS
        ownership (
            id   INTEGER PRIMARY KEY,
            zoneid INTEGER NOT NULL,
            userid INTEGER NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id),
            FOREIGN KEY(userid) REFERENCES users(id)
        )"#,
    )
    .execute(&mut tx)
    .await?;

    #[cfg(test)]
    eprintln!("Ensuring DB Ownership index exists");
    log::debug!("Ensuring DB Ownership index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_ownership
        ON ownership (
            zoneid,
            userid
        )",
    )
    .execute(&mut tx)
    .await?;

    tx.commit().await
}

pub async fn create_zones_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await.unwrap();

    log::debug!("Ensuring DB Zones table exists");
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS
        zones (
            id   INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            rname TEXT NOT NULL,
            serial INTEGER NOT NULL,
            refresh INTEGER NOT NULL,
            retry INTEGER NOT NULL,
            expire INTEGER NOT NULL,
            minimum INTEGER NOT NULL
        )"#,
    )
    .execute(&mut tx)
    .await?;

    // .execute(tx).await;
    log::debug!("Ensuring DB Records index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_zones
        ON zones (
            id,name
        )",
    )
    .execute(&mut tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn create_records_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    log::debug!("Ensuring DB Records table exists");

    let mut tx = pool.begin().await.unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS
        records (
            id      INTEGER PRIMARY KEY,
            zoneid  INTEGER NOT NULL,
            name    TEXT, /* this can be null for apex records */
            ttl     INTEGER,
            rrtype  INTEGER NOT NULL,
            rclass  INTEGER NOT NULL,
            rdata   TEXT NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id)
        )",
    )
    .execute(&mut tx)
    .await?;
    log::debug!("Ensuring DB Records index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_records
        ON records (
            id,zoneid,name,rrtype,rclass
        )",
    )
    .execute(&mut tx)
    .await?;
    log::debug!("Ensuring DB Records view exists");
    // this view lets us query based on the full name
    sqlx::query(
        &format!("CREATE VIEW IF NOT EXISTS {} ( record_id, zone_id, rrtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zone_id, records.rrtype, records.rclass ,records.rdata,
        CASE
            WHEN records.name is NULL THEN zones.name
            ELSE records.name || '.' || zones.name
        END AS name,
        CASE WHEN records.ttl is NULL then zones.minimum
            WHEN records.ttl > zones.minimum THEN records.ttl
            ELSE records.ttl
        END AS ttl
        from records, zones where records.zoneid = zones.id", SQL_VIEW_RECORDS)
    ).execute(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

// pub async fn create_zone_with_conn(
//     conn: &mut SqliteConnection,
//     zone: FileZone,
// ) -> Result<u64, sqlx::Error> {

//     zone.create
//     let mut args = SqliteArguments::default();
//     let serial = zone.serial.to_string();
//     let refresh = zone.refresh.to_string();
//     let retry = zone.retry.to_string();
//     let expire = zone.expire.to_string();
//     let minimum = zone.minimum.to_string();
//     for arg in [
//         &zone.name,
//         &zone.rname,
//         &serial,
//         &refresh,
//         &retry,
//         &expire,
//         &minimum,
//     ] {
//         args.add(arg);
//     }

//     let result = sqlx::query_with(
//         "INSERT INTO zones (name, rname, serial, refresh, retry, expire, minimum)
//             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
//         args,
//     )
//     .execute(conn)
//     .await?;
//     Ok(result.rows_affected())
// }

// /// create a resource record within a zone
// pub async fn create_record(pool: &SqlitePool, record: FileZoneRecord) -> Result<u64, sqlx::Error> {
//     let mut txn = pool.begin().await?;
//     let res = create_record_with_conn(&mut txn, record).await?;
//     txn.commit().await?;
//     Ok(res)
// }

// /// create a resource record within a zone
// pub async fn create_record_with_conn(
//     txn: &mut Transaction<'_, Sqlite>,
//     record: FileZoneRecord,
// ) -> Result<u64, sqlx::Error> {
//     let rclass: u16 = record.class as u16;
//     let rrtype = RecordType::from(record.rrtype);
//     let rrtype = rrtype as u16;

//     let record_name = match record.name.len() {
//         0 => None,
//         _ => Some(record.name),
//     };

//     let mut args = SqliteArguments::default();
//     let input_args: Vec<Option<String>> = vec![
//         Some(record.zoneid.to_string()),
//         record_name,
//         Some(record.ttl.to_string()),
//         Some(rrtype.to_string()),
//         Some(rclass.to_string()),
//         Some(record.rdata),
//     ];
//     for arg in input_args {
//         args.add(arg);
//     }
//     let result = sqlx::query_with(
//         "INSERT INTO records (zoneid, name, ttl,rrtype, rclass, rdata)
//                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
//         args,
//     )
//     .execute(&mut *txn)
//     .await?;
//     Ok(result.rows_affected())
// }

/// Query the zones table, name_or_id can be the zoneid or the name - if they match then you're bad and you should feel bacd.

pub async fn get_zone_with_txn(
    txn: &mut Transaction<'_, Sqlite>,
    name_or_id: &str,
) -> Result<Option<FileZone>, sqlx::Error> {
    // let mut args = SqliteArguments::default();
    // args.add(name);

    let result = sqlx::query(
        "SELECT
        id, name, rname, serial, refresh, retry, expire, minimum
        FROM zones
        WHERE name = ? or id = ? LIMIT 1",
    )
    .bind(name_or_id)
    .bind(name_or_id)
    .fetch_optional(&mut *txn)
    .await?;
    let mut zone = match result {
        None => return Ok(None),
        Some(row) => {
            let id: i64 = row.get(0);
            #[cfg(test)]
            eprintln!("Building FileZone");
            FileZone {
                id: id as u64,
                name: row.get(1),
                rname: row.get(2),
                serial: row.get(3),
                refresh: row.get(4),
                retry: row.get(5),
                expire: row.get(6),
                minimum: row.get(7),
                records: vec![],
            }
        }
    };

    let result = sqlx::query(
        "SELECT
        id, zoneid, name, ttl, rrtype, rclass, rdata
        FROM records
        WHERE zoneid = ?",
    )
    .bind(zone.id as i64)
    .fetch_all(&mut *txn)
    .await?;

    zone.records = result
        .into_iter()
        .filter_map(|r| match FileZoneRecord::try_from(r) {
            Ok(val) => Some(val),
            Err(_) => None,
        })
        .collect();
    Ok(Some(zone))
}

pub async fn get_zone(pool: &SqlitePool, name: String) -> Result<Option<FileZone>, sqlx::Error> {
    let mut txn = pool.begin().await?;

    get_zone_with_txn(&mut txn, &name).await
}

#[allow(dead_code)]
pub async fn update_zone(pool: &SqlitePool, zone: FileZone) -> Result<u64, sqlx::Error> {
    let mut txn = pool.begin().await?;
    let res = update_zone_with_conn(&mut txn, zone).await?;
    txn.commit().await?;
    Ok(res)
}

#[allow(dead_code)]
pub async fn update_zone_with_conn(
    txn: &mut Transaction<'_, Sqlite>,
    zone: FileZone,
) -> Result<u64, sqlx::Error> {
    // let mut tx = pool.begin().await?;

    let mut args = SqliteArguments::default();
    args.add(zone.rname);
    args.add(zone.serial);
    args.add(zone.refresh);
    args.add(zone.retry);
    args.add(zone.expire);
    args.add(zone.minimum);
    args.add(zone.id as i64);

    let qry = sqlx::query_with(
        "UPDATE zones
        set rname = ?, serial = ?, refresh = ?, retry = ?, expire = ?, minimum =?
        WHERE id = ?",
        args,
    )
    .execute(&mut *txn)
    .await?;
    println!("Rows updated: {}", qry.rows_affected());
    Ok(qry.rows_affected())
}

impl TryFrom<SqliteRow> for InternalResourceRecord {
    type Error = String;
    fn try_from(row: SqliteRow) -> Result<InternalResourceRecord, String> {
        let record_id: i64 = row.get(0);
        let zoneid: i64 = row.get(1);
        let zoneid: u64 = zoneid.try_into().unwrap_or(0);
        let record_name: String = row.get(2);
        let record_class: u16 = row.get(3);
        let record_type: u16 = row.get(4);
        let rrtype: &str = RecordType::from(&record_type).into();
        let rdata: String = row.get(5);
        let ttl: u32 = row.get(6);
        InternalResourceRecord::try_from(FileZoneRecord {
            name: record_name,
            ttl,
            zoneid,
            id: record_id as u64,
            rrtype: rrtype.to_string(),
            class: RecordClass::from(&record_class),
            rdata,
        })
    }
}

pub async fn get_records(
    conn: &Pool<Sqlite>,
    name: String,
    rrtype: RecordType,
    rclass: RecordClass,
) -> Result<Vec<InternalResourceRecord>, sqlx::Error> {
    let res = sqlx::query(&format!(
        "SELECT
        record_id, zone_id, name, rclass, rrtype, rdata, ttl
        FROM {}
        WHERE name = ? AND rrtype = ? AND rclass = ?",
        SQL_VIEW_RECORDS
    ))
    .bind(&name)
    .bind(rrtype as u16)
    .bind(rclass)
    .fetch_all(&mut conn.acquire().await?)
    .await?;

    if res.is_empty() {
        eprintln!("No results returned for {name} {rrtype} {rclass}");
        log::trace!("No results returned for {name} ");
    }

    let mut results: Vec<InternalResourceRecord> = vec![];
    for row in res {
        #[cfg(test)]
        eprintln!("Checking row => IRR");
        if let Ok(irr) = InternalResourceRecord::try_from(row) {
            results.push(irr);
        }
    }
    log::trace!("results: {results:?}");
    Ok(results)
}

impl FileZone {
    ///Hand it a filezone and it'll update the things
    // pub async fn load_zone(
    //     &self,
    //     mut conn: PoolConnection<Sqlite>,
    // ) -> Result<u64, sqlx::Error> {
    //     let mut txn = conn.begin().await?;
    //     let res = self.create_with_txn(&mut txn).await?;

    //     txn.commit().await?;
    //     Ok(res)
    // }
    pub async fn get_zone_records(
        &self,
        txn: &mut Transaction<'_, Sqlite>,
    ) -> Result<Vec<FileZoneRecord>, sqlx::Error> {
        let res = sqlx::query(
            "SELECT
            id, zoneid, name, ttl, rrtype, rclass, rdata
            FROM records
            WHERE zoneid = ?",
        )
        .bind(self.id as i64)
        .fetch_all(&mut *txn)
        .await?;

        if res.is_empty() {
            log::trace!("No results returned for zone_id={}", self.id);
        }

        let mut results: Vec<FileZoneRecord> = vec![];
        for row in res {
            if let Ok(irr) = FileZoneRecord::try_from(row) {
                results.push(irr);
            }
        }

        log::trace!("results: {results:?}");
        Ok(results)
    }
}

/// export a zone!
pub async fn export_zone(
    mut conn: PoolConnection<Sqlite>,
    zone_id: u64,
) -> Result<FileZone, sqlx::Error> {
    #[cfg(test)]
    println!("Started export_zone");
    let mut txn = conn.begin().await?;

    let mut zone = match get_zone_with_txn(&mut txn, &zone_id.to_string()).await? {
        None => {
            #[cfg(test)]
            println!("Couldn't find zone with id: {zone_id}");
            return Err(sqlx::Error::RowNotFound);
        }
        Some(value) => value,
    };
    #[cfg(test)]
    println!("Getting zone records...");
    let zone_records = zone.get_zone_records(&mut txn).await?;
    zone.records = zone_records;
    Ok(zone)
}

pub async fn export_zone_json(
    conn: PoolConnection<Sqlite>,
    zone_id: u64,
) -> Result<String, String> {
    let zone = export_zone(conn, zone_id)
        .await
        .map_err(|e| format!("{e:?}"))?;

    zone.json()
}

#[async_trait]
impl DBEntity for FileZone {
    fn table_name(&self) -> &str {
        "zones"
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(1)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<u64, sqlx::Error> {
        // check the zone exists
        let find_zone = match get_zone_with_txn(txn, &self.name).await {
            Ok(val) => {
                log::trace!("Found existing zone");
                val
            }
            Err(err) => {
                // failed to query the DB
                return Err(err);
            }
        };

        let updated_zone: FileZone = match find_zone {
            None => {
                // if it's new, add it
                #[cfg(test)]
                eprintln!("Creating zone {self:?}");
                log::debug!("Creating zone {self:?}");

                // zone.create
                let mut args = SqliteArguments::default();
                let serial = self.serial.to_string();
                let refresh = self.refresh.to_string();
                let retry = self.retry.to_string();
                let expire = self.expire.to_string();
                let minimum = self.minimum.to_string();
                for arg in [
                    &self.name,
                    &self.rname,
                    &serial,
                    &refresh,
                    &retry,
                    &expire,
                    &minimum,
                ] {
                    args.add(arg);
                }

                sqlx::query_with(
                    "INSERT INTO zones (name, rname, serial, refresh, retry, expire, minimum)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    args,
                )
                .execute(&mut *txn)
                .await?;

                #[cfg(test)]
                eprintln!("Done creating zone");
                get_zone_with_txn(txn, &self.name).await?.unwrap()
            }
            Some(ez) => {
                if !self.matching_data(&ez) {
                    // update it if it's wrong

                    #[cfg(test)]
                    eprintln!("Updating zone");
                    log::debug!("Updating zone");
                    let mut new_zone = self.clone();
                    new_zone.id = ez.id;

                    let updated = update_zone_with_conn(txn, new_zone).await?;
                    #[cfg(test)]
                    eprintln!("Updated: {:?} record", updated);
                    log::debug!("Updated: {:?} record", updated);
                }
                get_zone_with_txn(txn, &self.name).await.unwrap().unwrap()
            }
        };
        #[cfg(test)]
        eprintln!("Zone after update: {updated_zone:?}");
        log::trace!("Zone after update: {updated_zone:?}");

        // drop all the records
        let mut args = SqliteArguments::default();
        args.add(updated_zone.id as i64);

        #[cfg(test)]
        eprintln!("Dropping all records for zone {self:?}");
        log::debug!("Dropping all records for zone {self:?}");
        sqlx_core::query::query_with("delete from records where zoneid = ?", args)
            .execute(&mut *txn)
            .await?;

        // add the records
        for mut record in self.records.clone() {
            #[cfg(test)]
            eprintln!("Creating new zone record: {record:?}");
            log::trace!("Creating new zone record: {record:?}");
            record.zoneid = updated_zone.id;
            if record.name == "@" {
                record.name = "".to_string();
            }
            record.save_with_txn(txn).await?;
        }
        #[cfg(test)]
        println!("Done creating zone!");
        Ok(1u64)
    }

    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(1)
    }
    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<u64, sqlx::Error> {
        let zone_id = self.id as i64;
        // delete all the records
        sqlx::query("DELETE FROM records where zoneid = ?")
            .bind(zone_id)
            .execute(&mut *txn)
            .await?;

        // delete all the ownership records
        sqlx::query("DELETE FROM ownership where zoneid = ?")
            .bind(zone_id)
            .execute(&mut *txn)
            .await?;

        // finally delete the zone
        let query = format!("DELETE FROM {} where id = ?", self.table_name());
        sqlx::query(&query).bind(zone_id).execute(&mut *txn).await?;

        Ok(1)
    }
}

#[async_trait]
pub trait DBEntity: Send {
    fn table_name(&self) -> &str;

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error>;
    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<u64, sqlx::Error>;

    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error>;
    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<u64, sqlx::Error>;

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl DBEntity for FileZoneRecord {
    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }

    fn table_name(&self) -> &str {
        "records"
    }

    async fn save(&self, pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error> {
        #[cfg(test)]
        eprintln!("Starting save");
        let mut txn = pool.begin().await?;
        let res = &self.save_with_txn(&mut txn).await?;
        match txn.commit().await {
            Err(err) => {
                eprintln!("Failed to commit transaction: {err:?}");
                return Err(err);
            }
            Ok(_) => eprintln!("Successfully saved {self:?} to the db"),
        };
        Ok(res.to_owned())
    }

    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<u64, sqlx::Error> {
        #[cfg(test)]
        eprintln!("Starting save_with_txn");

        // TODO: check if there's an existing one
        let record_name = match self.name.len() {
            0 => None,
            _ => Some(self.to_owned().name),
        };
        let existing_record = sqlx::query("SELECT id, zoneid, name, ttl, rrtype, rclass, rdata from records WHERE
        id = ? AND  zoneid = ? AND  name = ? AND  ttl = ? AND  rrtype = ? AND  rclass = ? AND rdata = ? LIMIT 1")
            .bind(self.id as i64)
            .bind(self.zoneid as i64)
            .bind(&record_name)
            .bind(self.ttl)
            .bind(RecordType::from(self.rrtype.clone()))
            .bind(self.class)
            .bind(self.rdata.to_string())
            .fetch_optional(&mut *txn).await?;

        let mut args = SqliteArguments::default();
        args.add(self.zoneid as i64);
        args.add(record_name);
        args.add(self.ttl);
        args.add(RecordType::from(self.rrtype.clone()));
        args.add(self.class);
        args.add(self.clone().rdata);

        if let Some(er) = &existing_record {
            let id: i64 = er.get("id");
            args.add(id);
        }

        let query = match existing_record {
            Some(_) => {
                #[cfg(test)]
                eprintln!("Found an existing record while saving!");
                sqlx::query_with(
                    "UPDATE records set zoneid = ?1, name = ?2, ttl = ?3, rrtype = ?4, rclass = ?5, rdata = ?6
                            WHERE id =?
                        ",
                    args,
                )
            }
            None => sqlx::query_with(
                "INSERT INTO records (zoneid, name, ttl, rrtype, rclass, rdata)
                            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                        ",
                args,
            ),
        };
        #[cfg(test)]
        println!("Saving record...");
        let result = query.execute(&mut *txn).await?;
        #[cfg(test)]
        eprintln!(
            "Finished fzr save_with_txn, wrote {} rows",
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }

    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<u64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await
    }

    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(format!("DELETE FROM {} WHERE id = ?", &self.table_name()).as_str())
            .bind(self.id as i64)
            .execute(&mut *txn)
            .await?;
        Ok(res.rows_affected())
    }
}

pub async fn get_zones_with_txn(
    txn: &mut Transaction<'_, Sqlite>,
    lim: i64,
    offset: i64,
) -> Result<Vec<FileZone>, sqlx::Error> {
    let result = sqlx::query(
        "SELECT
        id, name, rname, serial, refresh, retry, expire, minimum
        FROM zones
        LIMIT ? OFFSET ? ",
    )
    .bind(lim)
    .bind(offset)
    .fetch_all(&mut *txn)
    .await?;

    let rows: Vec<FileZone> = result
        .iter()
        .map(|row| {
            let id: i64 = row.get(0);
            #[cfg(test)]
            eprintln!("Building FileZone");
            FileZone {
                id: id as u64,
                name: row.get(1),
                rname: row.get(2),
                serial: row.get(3),
                refresh: row.get(4),
                retry: row.get(5),
                expire: row.get(6),
                minimum: row.get(7),
                records: vec![],
            }
        })
        .collect();
    Ok(rows)

    // let result = sqlx::query(
    //     "SELECT
    //     id, zoneid, name, ttl, rrtype, rclass, rdata
    //     FROM records
    //     WHERE zoneid = ?",
    // )
    // .bind(zone.id as i64)
    // .fetch_all(&mut *txn)
    // .await?;

    // zone.records = result
    //     .into_iter()
    //     .filter_map(|r| match FileZoneRecord::try_from(r) {
    //         Ok(val) => Some(val),
    //         Err(_) => None,
    //     })
    //     .collect();
    // Ok(Some(zone))
}

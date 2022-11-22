use crate::resourcerecord::SetTTL;
use std::io::ErrorKind;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::config::ConfigFile;
use crate::enums::{RecordClass, RecordType};

use crate::resourcerecord::InternalResourceRecord;
use crate::zones::{FileZone, FileZoneRecord};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use concread::cowcell::asynch::CowCellReadTxn;
use openidconnect::SubjectIdentifier;
use serde::{Deserialize, Serialize};
use sqlx::pool::PoolConnection;
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions, SqliteRow};
use sqlx::{
    Arguments, ConnectOptions, Connection, FromRow, Pool, Row, Sqlite, SqlitePool, Transaction,
};
use tokio::time;

#[cfg(test)]
pub mod test;

const SQL_VIEW_RECORDS: &str = "records_merged";

pub async fn get_conn(
    config_reader: CowCellReadTxn<ConfigFile>,
) -> Result<Pool<Sqlite>, std::io::Error> {
    let db_path: &str = &shellexpand::full(&config_reader.sqlite_path).unwrap();
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    log::debug!("Opening Database: {db_url}");

    let mut options = match SqliteConnectOptions::from_str(&db_url) {
        Ok(value) => value,
        Err(error) => {
            return Err(std::io::Error::new(
                ErrorKind::Other,
                format!("connection failed: {error:?}"),
            ))
        }
    };
    if config_reader.sql_log_statements {
        options.log_statements(log::LevelFilter::Trace);
    } else {
        options.log_statements(log::LevelFilter::Off);
    }
    // log anything that takes longer than 1s
    options.log_slow_statements(
        log::LevelFilter::Warn,
        Duration::from_secs(config_reader.sql_log_slow_duration),
    );

    match SqlitePool::connect_with(options).await {
        Ok(value) => Ok(value),
        Err(err) => Err(std::io::Error::new(
            ErrorKind::Other,
            format!("Error opening SQLite DB ({db_url:?}): {err:?}"),
        )),
    }
}

/// Do the basic setup and checks (if we write any)
pub async fn start_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    FileZone::create_table(pool).await?;
    User::create_table(pool).await?;
    UserAuthToken::create_table(pool).await?;
    FileZoneRecord::create_table(pool).await?;
    ZoneOwnership::create_table(pool).await?;
    log::info!("Completed DB Startup!");
    Ok(())
}

#[derive(Clone, Deserialize, Serialize, Debug, sqlx::FromRow)]
pub struct User {
    pub id: Option<i64>,
    pub displayname: String,
    pub username: String,
    pub email: String,
    // #[serde(default)]
    // pub owned_zones: Vec<i64>,
    pub disabled: bool,
    pub authref: Option<String>,
    pub admin: bool,
}

impl Default for User {
    fn default() -> Self {
        User {
            id: None,
            displayname: "Anonymous Kid".to_string(),
            username: "".to_string(),
            email: "".to_string(),
            disabled: true,
            authref: None,
            admin: false,
        }
    }
}

impl User {
    #[allow(dead_code, unused_variables)]
    pub async fn create(&self, pool: &SqlitePool, disabled: bool) -> Result<usize, sqlx::Error> {
        // TODO: test user create
        let res =
            sqlx::query("INSERT into users (username, email, disabled, authref) VALUES(?, ?, ?)")
                .bind(&self.username)
                .bind(&self.email)
                .bind(disabled)
                .bind(&self.authref)
                .execute(&mut pool.acquire().await?)
                .await?;

        Ok(1)
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // TODO: test user delete
        let mut txn = pool.begin().await?;

        let res = sqlx::query("DELETE FROM ownership WHERE userid = ?")
            .bind(self.id.unwrap() as i64)
            .execute(&mut *txn)
            .await?;

        let res = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(self.id.unwrap() as i64)
            .execute(&mut *txn)
            .await?;

        txn.commit().await?;

        Ok(())
    }

    // Query the DB looking for a user
    pub async fn get_by_email(pool: &SqlitePool, email: String) -> Result<Self, sqlx::Error> {
        let res = sqlx::query(
            "
            select * from users
            where email = ?
            ",
        )
        .bind(email)
        .fetch_one(pool)
        .await?;

        Ok(User::from(res))
    }

    /// Query the DB looking for a user
    pub async fn get_by_subject(
        pool: &SqlitePool,
        subject: &SubjectIdentifier,
    ) -> Result<Self, sqlx::Error> {
        let res = sqlx::query(
            "
            select * from users
            where authref = ?
            ",
        )
        .bind(subject.to_string())
        .fetch_one(pool)
        .await?;

        Ok(User::from(res))
    }

    pub async fn get_zones_for_user(
        &self,
        txn: &mut Transaction<'_, Sqlite>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<FileZone>, sqlx::Error> {
        let query_string = match self.admin {
            true => {
                "SELECT *
                    FROM zones
                    LIMIT ?1 OFFSET ?2"
            }
            false => {
                "SELECT *
                    FROM zones, ownership
                    WHERE zones.id = ownership.zoneid
                        AND ownership.userid = ?3
                    LIMIT ?1 OFFSET ?2"
            }
        };
        log::trace!(
            "get_zones_for_user query: {:?}",
            query_string.replace('\n', "")
        );
        let query = sqlx::query(query_string).bind(limit).bind(offset);
        let query = match self.admin {
            true => query,
            false => query.bind(self.id),
        };
        let rows: Vec<FileZone> = match query.fetch_all(txn).await {
            Err(error) => {
                log::error!("Error: {error:?}");
                vec![]
            }
            Ok(rows) => rows.iter().map(|row| row.into()).collect(),
        };
        Ok(rows)
    }

    pub async fn get_token(
        pool: &mut Pool<Sqlite>,
        tokenkey: &String,
    ) -> Result<TokenSearchRow, sqlx::Error> {
        let mut txn = pool.begin().await?;
        let res: TokenSearchRow = sqlx::query_as(
            "SELECT users.id as userid, users.displayname,  users.username, users.authref, users.email, users.disabled, users.authref, users.admin, tokenhash, tokenkey
            FROM user_tokens, users
            WHERE
                users.disabled=0 AND
                user_tokens.tokenkey=? AND
                user_tokens.userid=users.id")
            .bind(tokenkey)
            .fetch_one(&mut txn).await?;

        Ok(res)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenSearchRow {
    pub tokenhash: String,
    pub user: User,
    pub tokenkey: String,
}

impl FromRow<'_, SqliteRow> for TokenSearchRow {
    fn from_row(input: &SqliteRow) -> Result<Self, sqlx::Error> {
        let user = User {
            id: input.get("userid"),
            displayname: input.get("displayname"),
            username: input.get("username"),
            email: input.get("email"),
            disabled: input.get("disabled"),
            authref: input.get("authref"),
            admin: input.get("admin"),
        };
        Ok(TokenSearchRow {
            tokenkey: input.get("tokenkey"),
            tokenhash: input.get("tokenhash"),
            user,
        })
    }
}

impl From<SqliteRow> for User {
    fn from(row: SqliteRow) -> Self {
        let id: i64 = row.get("id");
        let displayname: String = row.get("displayname");
        let username: String = row.get("username");
        let email: String = row.get("email");
        let disabled: bool = row.get("disabled");
        let authref: Option<String> = row.get("authref");
        let admin: bool = row.get("admin");
        User {
            id: Some(id),
            displayname,
            username,
            email,
            disabled,
            authref,
            admin,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ZoneOwnership {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub userid: i64,
    pub zoneid: i64,
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
    pub async fn delete(&self, pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        // TODO: test ownership delete
        let res = sqlx::query("DELETE FROM ownership WHERE zoneid = ? AND userid = ?")
            .bind(self.zoneid as i64)
            .bind(self.userid as i64)
            .execute(&mut pool.acquire().await?)
            .await?;
        Ok(res.rows_affected() as i64)
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete_for_user(self, pool: &SqlitePool) -> Result<User, sqlx::Error> {
        // TODO: test user delete
        // TODO: delete all ownership records
        todo!();
    }

    // get the thing by the other thing
    pub async fn get_ownership_by_userid<'t>(
        txn: &mut Transaction<'t, Sqlite>,
        userid: &i64,
        zoneid: &i64,
    ) -> Result<ZoneOwnership, sqlx::Error> {
        let res = sqlx::query(
            "select users.username, zones.name, zones.id as zoneid, ownership.id as id, userid
        from users, ownership, zones
        where ownership.userid = ? AND ownership.zoneid = ? AND (ownership.userid = users.id AND
            users.disabled=0 and
            (zones.id = ownership.zoneid OR
            users.admin=1
            ))",
        )
        .bind(userid)
        .bind(zoneid)
        .fetch_one(txn)
        .await?;

        Ok(res.into())
    }
}

/// Query the zones table, name_or_id can be the zoneid or the name - if they match then you're bad and you should feel bad.
pub async fn get_zone_with_txn(
    txn: &mut Transaction<'_, Sqlite>,
    id: Option<i64>,
    name: Option<String>,
) -> Result<Option<FileZone>, sqlx::Error> {
    // let mut args = SqliteArguments::default();
    // args.add(name);

    let result = sqlx::query(
        "SELECT
        id, name, rname, serial, refresh, retry, expire, minimum
        FROM zones
        WHERE name = ? or id = ? LIMIT 1",
    )
    .bind(name)
    .bind(id)
    .fetch_optional(&mut *txn)
    .await?;
    let mut zone = match result {
        None => return Ok(None),
        Some(row) => {
            #[cfg(test)]
            eprintln!("Building FileZone");
            FileZone {
                id: row.get("id"),
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

impl TryFrom<SqliteRow> for InternalResourceRecord {
    type Error = String;
    fn try_from(row: SqliteRow) -> Result<InternalResourceRecord, String> {
        let record_id: i64 = row.get(0);
        let record_class: u16 = row.get(3);
        let record_type: u16 = row.get(4);
        let rrtype: &str = RecordType::from(&record_type).into();
        let rdata: String = row.get(5);
        let ttl: u32 = row.get(6);
        InternalResourceRecord::try_from(FileZoneRecord {
            name: row.get("name"),
            ttl,
            zoneid: row.get("zoneid"),
            id: record_id,
            rrtype: rrtype.to_string(),
            class: RecordClass::from(&record_class),
            rdata,
        })
    }
}

/// Pull a vec of [InternalResourceRecord]s directly from the database
///
/// Setting normalize_ttls=true sets the TTL on all records to the LOWEST of the returned records.
pub async fn get_records(
    conn: &Pool<Sqlite>,
    name: String,
    rrtype: RecordType,
    rclass: RecordClass,
    normalize_ttls: bool,
) -> Result<Vec<InternalResourceRecord>, sqlx::Error> {
    let res = sqlx::query(&format!(
        "SELECT
        record_id, zoneid, name, rclass, rrtype, rdata, ttl
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
        if let Ok(irr) = InternalResourceRecord::try_from(row) {
            results.push(irr);
        }
    }

    // skip the normalisation step if we've got 0 or 1 result.
    if results.len() <= 1 {
        return Ok(results);
    }

    let results = match normalize_ttls {
        true => {
            let min_ttl = results.iter().map(|r| r.ttl()).min();
            let min_ttl = match min_ttl {
                Some(val) => val.to_owned(),
                None => {
                    log::error!("Somehow failed to get minimum TTL from query");
                    1
                }
            };

            results
                .to_vec()
                .iter()
                .map(|r| r.clone().set_ttl(min_ttl))
                .collect()
        }
        false => results,
    };
    Ok(results)
}

impl FileZone {
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
            log::trace!("No results returned for zoneid={}", self.id);
        }

        // let mut results: Vec<FileZoneRecord> = vec![];
        // for row in res {
        //     if let Ok(irr) = FileZoneRecord::try_from(row) {
        //         results.push(irr);
        //     }
        // }
        let results: Vec<FileZoneRecord> = res
            .iter()
            .filter_map(|r| FileZoneRecord::try_from(r).ok())
            .collect();

        log::trace!("results: {results:?}");
        Ok(results)
    }

    pub async fn get_orphans(pool: &SqlitePool) -> Result<Vec<FileZone>, sqlx::Error> {
        let res = sqlx::query(
            "
            SELECT name, userid from zones
            LEFT OUTER JOIN ownership on zones.id = ownership.zoneid
            where userid IS NULL",
        )
        .fetch_all(&mut pool.acquire().await?)
        .await?;
        let res: Vec<FileZone> = res.into_iter().map(|r| r.into()).collect();
        Ok(res)
    }
}

/// export a zone!
pub async fn export_zone(
    mut conn: PoolConnection<Sqlite>,
    zoneid: i64,
) -> Result<FileZone, sqlx::Error> {
    #[cfg(test)]
    println!("Started export_zone");
    let mut txn = conn.begin().await?;

    let mut zone = match get_zone_with_txn(&mut txn, Some(zoneid), None).await? {
        None => {
            #[cfg(test)]
            println!("Couldn't find zone with id: {zoneid}");
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

pub async fn export_zone_json(conn: PoolConnection<Sqlite>, zoneid: i64) -> Result<String, String> {
    let zone = export_zone(conn, zoneid)
        .await
        .map_err(|e| format!("{e:?}"))?;

    zone.json()
}

#[async_trait]
pub trait DBEntity: Send {
    const TABLE: &'static str;

    async fn create_table(pool: &SqlitePool) -> Result<(), sqlx::Error>;

    /// Get the entity
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Arc<Self>, sqlx::Error>;
    async fn get_with_txn<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _id: &i64,
    ) -> Result<Arc<Self>, sqlx::Error>;
    async fn get_by_name<'t>(
        txn: &mut Transaction<'t, Sqlite>,
        name: &str,
    ) -> Result<Arc<Self>, sqlx::Error>;
    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, sqlx::Error>;
    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error>;
    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error>;
    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error>;
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error>;

    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error>;
    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<i64, sqlx::Error>;

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl DBEntity for FileZone {
    const TABLE: &'static str = "zones";

    async fn create_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
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

    /// Get by id
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Arc<Self>, sqlx::Error> {
        let mut txn = pool.begin().await?;
        Self::get_with_txn(&mut txn, &id).await
    }

    async fn get_with_txn<'t>(
        txn: &mut Transaction<'t, Sqlite>,
        id: &i64,
    ) -> Result<Arc<Self>, sqlx::Error> {
        let res = sqlx::query(
            "SELECT
            *
            FROM zones
            WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_one(&mut *txn)
        .await?;
        let mut zone: FileZone = res.into();
        log::debug!("got a zone: {zone:?}");

        let records = sqlx::query(
            "SELECT
            id, zoneid, name, ttl, rrtype, rclass, rdata
            FROM records
            WHERE zoneid = ?",
        )
        .bind(zone.id as i64)
        .fetch_all(txn)
        .await?;

        zone.records = records
            .into_iter()
            .filter_map(|r| match FileZoneRecord::try_from(r) {
                Ok(val) => Some(val),
                Err(_) => None,
            })
            .collect();
        Ok(Arc::new(zone))
    }

    async fn get_by_name<'t>(
        txn: &mut Transaction<'t, Sqlite>,
        name: &str,
    ) -> Result<Arc<Self>, sqlx::Error> {
        let res = sqlx::query(&format!("SELECT * from {} where name=?", Self::TABLE))
            .bind(name)
            .fetch_one(txn)
            .await?;

        let res: Self = res.into();
        Ok(Arc::new(res))
    }

    async fn get_all_user(
        _pool: &Pool<Sqlite>,
        _userid: i64,
    ) -> Result<Vec<Arc<Self>>, sqlx::Error> {
        todo!()
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        // check the zone exists
        let find_zone = match get_zone_with_txn(txn, None, Some(self.name.clone())).await {
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
                get_zone_with_txn(txn, None, Some(self.name.clone()))
                    .await?
                    .unwrap()
            }
            Some(ez) => {
                if !self.matching_data(&ez) {
                    // update it if it's wrong

                    #[cfg(test)]
                    eprintln!("Updating zone");
                    log::debug!("Updating zone");
                    let mut new_zone = self.clone();
                    new_zone.id = ez.id;

                    let updated = new_zone.update_with_txn(txn).await?;
                    #[cfg(test)]
                    eprintln!("Updated: {:?} record", updated);
                    log::debug!("Updated: {:?} record", updated);
                }
                get_zone_with_txn(txn, None, Some(self.name.clone()))
                    .await
                    .unwrap()
                    .unwrap()
            }
        };
        #[cfg(test)]
        eprintln!("Zone after update: {updated_zone:?}");
        log::trace!("Zone after update: {updated_zone:?}");

        // drop all the records
        #[cfg(test)]
        eprintln!("Dropping all records for zone {self:?}");
        // log::debug!("Dropping all records for zone {self:?}");
        sqlx::query("delete from records where zoneid = ?")
            .bind(updated_zone.id as i64)
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

        // TODO: this needs to be the zone id
        Ok(1)
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(
            "UPDATE zones
            set rname = ?, serial = ?, refresh = ?, retry = ?, expire = ?, minimum =?
            WHERE id = ?",
        )
        .bind(&self.rname)
        .bind(self.serial)
        .bind(self.refresh)
        .bind(self.retry)
        .bind(self.expire)
        .bind(self.minimum)
        .bind(self.id)
        .execute(txn)
        .await?;
        Ok(res.rows_affected() as i64)
    }
    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(1)
    }
    /// Delete the entity from the database, when you're in a transaction.
    ///
    /// This one happens in the order ownership -> records -> zone because at the very least,
    /// if it fails after the ownership thing, then non-admin users can't see the zone
    /// and admins will just have to clean it up manually
    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<i64, sqlx::Error> {
        // delete all the ownership records
        sqlx::query("DELETE FROM ownership where zoneid = ?")
            .bind(self.id)
            .execute(&mut *txn)
            .await?;

        // delete all the records
        sqlx::query("DELETE FROM records where zoneid = ?")
            .bind(self.id)
            .execute(&mut *txn)
            .await?;

        // finally delete the zone
        let query = format!("DELETE FROM {} where id = ?", FileZone::TABLE);
        sqlx::query(&query).bind(self.id).execute(&mut *txn).await?;

        Ok(1)
    }
}

impl From<&SqliteRow> for FileZone {
    fn from(input: &SqliteRow) -> Self {
        input.into()
    }
}

impl From<SqliteRow> for FileZone {
    fn from(input: SqliteRow) -> Self {
        FileZone {
            id: input.get("id"),
            name: input.get("name"),
            rname: input.get("rname"),
            serial: input.get("serial"),
            refresh: input.get("refresh"),
            retry: input.get("retry"),
            expire: input.get("expire"),
            minimum: input.get("minimum"),
            records: vec![], // can't fill this out yet
        }
    }
}

#[async_trait]
impl DBEntity for FileZoneRecord {
    const TABLE: &'static str = "records";

    async fn create_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
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
        &format!("CREATE VIEW IF NOT EXISTS {} ( record_id, zoneid, rrtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zoneid, records.rrtype, records.rclass ,records.rdata,
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

    /// Get by id
    async fn get(_pool: &Pool<Sqlite>, _id: i64) -> Result<Arc<Self>, sqlx::Error> {
        todo!();
        // get_records(conn, name, rrtype, rclass)
    }

    async fn get_with_txn<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _id: &i64,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }
    async fn get_by_name<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _name: &str,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }
    async fn get_all_user(
        _pool: &Pool<Sqlite>,
        _userid: i64,
    ) -> Result<Vec<Arc<Self>>, sqlx::Error> {
        todo!()
    }

    async fn save(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
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
    ) -> Result<i64, sqlx::Error> {
        #[cfg(test)]
        eprintln!("Starting save_with_txn for {self:?}");
        log::trace!("Starting save_with_txn for {self:?}");
        let record_name = match self.name.len() {
            0 => None,
            _ => Some(self.to_owned().name),
        };
        #[cfg(test)]
        eprintln!(
            "save_with_txn rtype: {} => {}",
            self.rrtype.clone(),
            RecordType::from(self.rrtype.clone())
        );
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
        Ok(result.rows_affected() as i64)
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await
    }

    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(format!("DELETE FROM {} WHERE id = ?", Self::TABLE).as_str())
            .bind(self.id)
            .execute(&mut *txn)
            .await?;
        Ok(res.rows_affected() as i64)
    }

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl DBEntity for ZoneOwnership {
    const TABLE: &'static str = "ownership";

    async fn create_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await.unwrap();

        #[cfg(test)]
        eprintln!("Ensuring DB {} table exists", Self::TABLE);
        log::debug!("Ensuring DB {} table exists", Self::TABLE);
        sqlx::query(&format!(
            r#"CREATE TABLE IF NOT EXISTS
                {} (
                    id   INTEGER PRIMARY KEY,
                    zoneid INTEGER NOT NULL,
                    userid INTEGER NOT NULL,
                    FOREIGN KEY(zoneid) REFERENCES zones(id),
                    FOREIGN KEY(userid) REFERENCES users(id)
                )"#,
            Self::TABLE
        ))
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

    /// Get an ownership record by its id
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Arc<Self>, sqlx::Error> {
        let mut conn = pool.acquire().await?;

        let res: ZoneOwnership =
            sqlx::query("SELECT id, zoneid, userid from ownership where id = ?")
                .bind(id)
                .fetch_one(&mut conn)
                .await?
                .into();
        Ok(Arc::new(res))
    }

    /// This getter is by zoneid, since it should return less results
    async fn get_with_txn<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _id: &i64,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }

    async fn get_by_name<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _name: &str,
    ) -> Result<Arc<Self>, sqlx::Error> {
        // TODO implement ZoneOwnership get_by_name which gets by zone name
        unimplemented!("Not applicable for this!");
    }
    /// Get an ownership record by its id
    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, sqlx::Error> {
        let mut conn = pool.acquire().await?;

        let res = sqlx::query("SELECT * from ownership where id = ?")
            .bind(id)
            .fetch_all(&mut conn)
            .await?;
        let result: Vec<Arc<ZoneOwnership>> = res.into_iter().map(|z| Arc::new(z.into())).collect();
        Ok(result)
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(&format!(
            "INSERT INTO {} (zoneid, userid) values ( ?, ? )",
            Self::TABLE
        ))
        .bind(self.zoneid)
        .bind(self.userid)
        .execute(txn)
        .await?;

        return Ok(res.last_insert_rowid());
    }

    /// create new, this just calls save_with_txn
    async fn create_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        self.save_with_txn(txn).await
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        unimplemented!("this should never be updated");
    }
    /// delete the entity from the database
    async fn delete(&self, _pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        todo!()
    }

    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(
        &self,
        _txn: &mut Transaction<'_, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}
impl From<SqliteRow> for ZoneOwnership {
    fn from(row: SqliteRow) -> Self {
        let id: i64 = row.get("id");
        let userid: i64 = row.get("userid");
        let zoneid: i64 = row.get("zoneid");

        ZoneOwnership {
            id: Some(id),
            zoneid,
            userid,
        }
    }
}

#[async_trait]
impl DBEntity for User {
    const TABLE: &'static str = "users";

    async fn create_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        log::debug!("Ensuring DB Users table exists");
        sqlx::query(&format!(
            r#"CREATE TABLE IF NOT EXISTS
        {} (
            id  INTEGER PRIMARY KEY,
            displayname TEXT NOT NULL,
            username TEXT NOT NULL,
            email TEXT NOT NULL,
            disabled BOOL NOT NULL,
            authref TEXT,
            admin BOOL DEFAULT 0
        )"#,
            Self::TABLE
        ))
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
    /// Get an ownership record by its id
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Arc<Self>, sqlx::Error> {
        let mut conn = pool.acquire().await?;

        let res: User = sqlx::query(&format!(
            "SELECT id, displayname, username, email, disabled from {} where id = ?",
            Self::TABLE
        ))
        .bind(id)
        .fetch_one(&mut conn)
        .await?
        .into();
        Ok(Arc::new(res))
    }
    async fn get_with_txn<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _id: &i64,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }
    async fn get_by_name<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _name: &str,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }
    /// Get an ownership record by its id, which is slightly ironic in this case
    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, sqlx::Error> {
        let mut conn = pool.acquire().await?;

        let res = sqlx::query(&format!(
            "SELECT id, zoneid, userid from {} where id = ?",
            Self::TABLE
        ))
        .bind(id)
        .fetch_all(&mut conn)
        .await?;
        let result: Vec<Arc<Self>> = res.into_iter().map(|z| Arc::new(z.into())).collect();
        Ok(result)
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(
            "INSERT INTO users
            (displayname, username, email, disabled, authref, admin)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&self.displayname)
        .bind(&self.username)
        .bind(&self.email)
        .bind(self.disabled)
        .bind(&self.authref)
        .bind(self.admin)
        .execute(txn)
        .await?;
        Ok(res.last_insert_rowid())
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }
    /// delete the entity from the database
    async fn delete(&self, _pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        todo!()
    }

    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(
        &self,
        _txn: &mut Transaction<'_, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAuthToken {
    pub id: Option<i64>,
    pub name: String,
    pub issued: DateTime<Utc>,
    pub expiry: Option<DateTime<Utc>>,
    pub userid: i64,
    pub tokenkey: String,
    pub tokenhash: String,
}

impl UserAuthToken {
    pub async fn get_authtoken(
        pool: &SqlitePool,
        tokenkey: String,
    ) -> Result<UserAuthToken, sqlx::Error> {
        let res = sqlx::query(&format!(
            "select id, issued, expiry, tokenkey, tokenhash, userid from {} where tokenkey = ?",
            Self::TABLE
        ))
        .bind(tokenkey)
        .fetch_one(&mut pool.acquire().await?)
        .await?;
        Ok(res.into())
    }

    pub async fn cleanup(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let current_time = Utc::now();
        log::debug!(
            "Starting cleanup of {} table for sessions expiring before {}",
            Self::TABLE,
            current_time.to_rfc3339()
        );

        match sqlx::query(&format!(
            "DELETE FROM {} where expiry NOT NULL and expiry < ?",
            Self::TABLE
        ))
        .bind(current_time.timestamp())
        .execute(&mut pool.acquire().await?)
        .await
        {
            Ok(res) => {
                log::info!(
                    "Cleanup of {} table complete, {} rows deleted.",
                    Self::TABLE,
                    res.rows_affected()
                );
                Ok(())
            }
            Err(error) => {
                log::error!(
                    "Failed to complete cleanup of {} table: {error:?}",
                    Self::TABLE
                );
                Err(error)
            }
        }
    }
}

#[async_trait]
impl DBEntity for UserAuthToken {
    const TABLE: &'static str = "user_tokens";

    async fn create_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let mut conn = pool.acquire().await?;
        log::debug!("Ensuring DB {} table exists", Self::TABLE);

        match sqlx::query(&format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
            Self::TABLE
        ))
        .fetch_optional(&mut conn)
        .await?
        {
            None => {
                log::debug!("Creating {} table", Self::TABLE);
                sqlx::query(&format!(
                    r#"CREATE TABLE IF NOT EXISTS
                    {} (
                        id INTEGER PRIMARY KEY,
                        name TEXT NOT NULL,
                        issued TEXT NOT NULL,
                        expiry TEXT,
                        tokenkey TEXT NOT NULL,
                        tokenhash TEXT NOT NULL,
                        userid INTEGER NOT NULL,
                        FOREIGN KEY(userid) REFERENCES users(id)
                    )"#,
                    Self::TABLE
                ))
                .execute(&mut conn)
                .await?;
            }
            Some(_) => {
                log::debug!("Updating the table");
                // get the columns in the table
                let res = sqlx::query(&format!("PRAGMA table_info({})", Self::TABLE))
                    .fetch_all(&mut conn)
                    .await?;

                let mut found_name = false;
                let mut found_tokenkey = false;
                for row in res.iter() {
                    let rowname: &str = row.get("name");
                    if rowname == "name" {
                        log::debug!("Found the name column in the {} table", Self::TABLE);
                        found_name = true;
                    }

                    let rowname: &str = row.get("name");
                    if rowname == "tokenkey" {
                        log::debug!("Found the tokenkey column in the {} table", Self::TABLE);
                        found_tokenkey = true;
                    }
                }

                if !found_name {
                    log::info!("Adding the name column to the {} table", Self::TABLE);
                    sqlx::query(&format!(
                        "ALTER TABLE \"{}\" ADD COLUMN name TEXT NOT NULL DEFAULT \"Token Name\"",
                        Self::TABLE
                    ))
                    .execute(&mut conn)
                    .await?;
                }
                if !found_tokenkey {
                    log::info!("Adding the tokenkey column to the {} table, this will drop the contents of the API tokens table, because of the format change.", Self::TABLE);

                    match dialoguer::Confirm::new()
                        .with_prompt("Please confirm that you want to take this action")
                        .interact()
                    {
                        Ok(value) => {
                            if !value {
                                return Err(sqlx::Error::Protocol("Cancelled".to_string()));
                            }
                        }
                        Err(error) => {
                            log::error!("Cancelled! {error:?}");
                            return Err(sqlx::Error::Protocol("Cancelled".to_string()));
                        }
                    };
                    sqlx::query(&format!("DELETE FROM {}", Self::TABLE))
                        .execute(&mut conn)
                        .await?;
                    sqlx::query(&format!(
                        "ALTER TABLE \"{}\" ADD COLUMN tokenkey TEXT NOT NULL DEFAULT \"old_tokenkey\"",
                        Self::TABLE
                    ))
                    .execute(&mut conn)
                    .await?;
                }
            }
        };

        match sqlx::query(&format!("DROP INDEX ind_{}_fields", Self::TABLE))
            .execute(&mut conn)
            .await
        {
            Ok(_) => log::trace!(
                "Didn't find  ind_{}_fields index, no action required",
                Self::TABLE
            ),
            Err(err) => match err {
                sqlx::Error::Database(zzz) => {
                    if zzz.message() != "no such index: ind_user_tokens_fields" {
                        log::error!("Database Error: {:?}", zzz);
                        panic!();
                    }
                }
                _ => {
                    log::error!("{err:?}");
                    panic!();
                }
            },
        };

        sqlx::query(&format!(
            "CREATE UNIQUE INDEX IF NOT EXISTS
        ind_{0}_findit
        ON {0} ( userid, tokenkey, tokenhash )",
            Self::TABLE
        ))
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    /// Get the entity
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Arc<Self>, sqlx::Error> {
        Ok(Arc::new(
            sqlx::query(&format!("SELECT * from {} where id = ?", Self::TABLE))
                .bind(id)
                .fetch_one(pool)
                .await?
                .into(),
        ))
    }

    async fn get_with_txn<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _id: &i64,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }
    // TODO: maybe get by name gets it by the username?
    async fn get_by_name<'t>(
        _txn: &mut Transaction<'t, Sqlite>,
        _name: &str,
    ) -> Result<Arc<Self>, sqlx::Error> {
        todo!()
    }

    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, sqlx::Error> {
        let res = sqlx::query(&format!("SELECT * from {} where userid = ?", Self::TABLE))
            .bind(id)
            .fetch_all(pool)
            .await?;
        let res: Vec<Arc<UserAuthToken>> = res
            .into_iter()
            .map(|r| Arc::new(UserAuthToken::from(r)))
            .collect();
        Ok(res)
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        let expiry = self.expiry.map(|v| v.timestamp());
        let issued = self.issued.timestamp();

        let res = sqlx::query(&format!(
            "INSERT INTO {} (name, issued, expiry, userid, tokenkey, tokenhash) VALUES (?, ?, ?, ?, ?, ?)",
            Self::TABLE
        ))
        .bind(&self.name)
        .bind(issued)
        .bind(expiry)
        .bind(self.userid)
        .bind(&self.tokenkey)
        .bind(&self.tokenhash)
        .execute(txn)
        .await?;
        Ok(res.last_insert_rowid())
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut Transaction<'t, Sqlite>,
    ) -> Result<i64, sqlx::Error> {
        todo!();
    }

    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<i64, sqlx::Error> {
        let mut txn = pool.begin().await?;
        let res = self.delete_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }
    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, txn: &mut Transaction<'_, Sqlite>) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(&format!("DELETE FROM {} where id = ?", &Self::TABLE))
            .bind(self.id)
            .execute(txn)
            .await?;
        Ok(res.rows_affected() as i64)
    }

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}

impl From<SqliteRow> for UserAuthToken {
    fn from(input: SqliteRow) -> Self {
        let expiry: Option<String> = input.get("expiry");
        let expiry: Option<DateTime<Utc>> = match expiry {
            None => None,
            Some(val) => {
                let expiry = chrono::NaiveDateTime::parse_from_str(&val, "%s").unwrap();
                let expiry: DateTime<Utc> = chrono::DateTime::from_utc(expiry, Utc);
                Some(expiry)
            }
        };

        let issued: String = input.get("issued");

        let issued = chrono::NaiveDateTime::parse_from_str(&issued, "%s").unwrap();
        let issued: DateTime<Utc> = chrono::DateTime::from_utc(issued, Utc);

        Self {
            id: input.get("id"),
            name: input.get("name"),
            issued,
            expiry,
            userid: input.get("userid"),
            tokenkey: input.get("tokenkey"),
            tokenhash: input.get("tokenhash"),
        }
    }
}

/// Run this periodically to clean up expired DB things
pub async fn cron_db_cleanup(pool: Pool<Sqlite>, period: Duration) {
    let mut interval = time::interval(period);
    loop {
        interval.tick().await;

        if let Err(error) = UserAuthToken::cleanup(&pool).await {
            log::error!("Failed to clean up UserAuthToken objects in DB cron: {error:?}");
        }
    }
}

pub async fn get_zones_with_txn(
    txn: &mut Transaction<'_, Sqlite>,
    lim: i64,
    offset: i64,
) -> Result<Vec<FileZone>, sqlx::Error> {
    let result = sqlx::query(
        "SELECT
        *
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
            #[cfg(test)]
            eprintln!("Building FileZone");
            row.into()
        })
        .collect();
    Ok(rows)
}

impl TryFrom<&SqliteRow> for FileZoneRecord {
    type Error = String;
    fn try_from(row: &SqliteRow) -> Result<Self, String> {
        row.to_owned().try_into()
    }
}

impl TryFrom<SqliteRow> for FileZoneRecord {
    type Error = String;
    fn try_from(row: SqliteRow) -> Result<Self, String> {
        let name: String = row.get("name");
        let rrtype: i32 = row.get("rrtype");
        let rrtype = RecordType::from(&(rrtype as u16));
        #[cfg(test)]
        eprintln!("rrtype: {rrtype:?}");
        let class: u16 = row.get("rclass");
        let rdata: String = row.get("rdata");
        let ttl: u32 = row.get("ttl");

        if let RecordType::ANY = rrtype {
            return Err("Cannot serve ANY records".to_string());
        }

        Ok(FileZoneRecord {
            zoneid: row.get("zoneid"),
            id: row.get("id"),
            name,
            rrtype: rrtype.to_string(),
            class: RecordClass::from(&class),
            rdata,
            ttl,
        })
    }
}

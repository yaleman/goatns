use crate::error::GoatNsError;
use crate::resourcerecord::SetTTL;
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
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions, SqliteRow};
use sqlx::{Arguments, ConnectOptions, FromRow, Pool, Row, Sqlite, SqliteConnection, SqlitePool};
use tokio::time;
use tracing::{debug, error, info, instrument, trace};

pub(crate) mod entities;
#[cfg(test)]
pub mod test;

const SQL_VIEW_RECORDS: &str = "records_merged";

/// Setup the database connection and pool
pub async fn get_conn(
    config_reader: CowCellReadTxn<ConfigFile>,
) -> Result<SqlitePool, GoatNsError> {
    let db_path: &str = &shellexpand::full(&config_reader.sqlite_path)
        .map_err(|err| GoatNsError::StartupError(err.to_string()))?;
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    debug!("Opening Database: {db_url}");

    let options = SqliteConnectOptions::from_str(&db_url)?;
    let options = if config_reader.sql_log_statements {
        options.log_statements(log::LevelFilter::Trace)
    } else {
        options.log_statements(log::LevelFilter::Off)
    };
    // log anything that takes longer than 1s
    let options = options.log_slow_statements(
        log::LevelFilter::Warn,
        Duration::from_secs(config_reader.sql_log_slow_duration),
    );

    SqlitePool::connect_with(options).await.map_err(|err| {
        error!("Error opening SQLite DB ({db_url:?}): {err:?}");
        err.into()
    })
}

/// Do the basic setup and checks (if we write any)
pub async fn start_db(pool: &SqlitePool) -> Result<(), GoatNsError> {
    FileZone::create_table(pool).await?;
    User::create_table(pool).await?;
    UserAuthToken::create_table(pool).await?;
    FileZoneRecord::create_table(pool).await?;
    ZoneOwnership::create_table(pool).await?;
    info!("Completed DB Startup!");
    Ok(())
}

#[derive(Clone, Deserialize, Serialize, Debug)]
/// DB Representation of a user
pub struct User {
    /// the user's ID
    pub id: Option<i64>,
    /// the user's display name
    pub displayname: String,
    /// the user's username
    pub username: String,
    /// the user's email address
    pub email: String,
    /// is the user disabled?
    pub disabled: bool,
    /// the user's authref from OIDC
    pub authref: Option<String>,
    /// is the user an admin?
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
    // Query the DB looking for a user
    // pub async fn get_by_email(pool: &SqlitePool, email: String) -> Result<Self, GoatNsError> {
    //     let res = sqlx::query(
    //         "
    //         select * from users
    //         where email = ?
    //         ",
    //     )
    //     .bind(email)
    //     .fetch_one(pool)
    //     .await?;

    //     Ok(User::from(res))
    // }

    /// Query the DB looking for a user
    pub async fn get_by_subject(
        pool: &SqlitePool,
        subject: &SubjectIdentifier,
    ) -> Result<Option<Self>, GoatNsError> {
        match sqlx::query(
            "
            select * from users
            where authref = ?
            ",
        )
        .bind(subject.to_string())
        .fetch_one(pool)
        .await
        {
            Ok(val) => Ok(Some(val.into())),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    #[instrument(skip(txn))]
    pub async fn get_zones_for_user(
        &self,
        txn: &mut SqliteConnection,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<FileZone>, GoatNsError> {
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
        trace!(
            "get_zones_for_user query: {:?}",
            query_string.replace('\n', "")
        );
        trace!("Building query");
        let query = sqlx::query(query_string).bind(limit).bind(offset);
        let query = match self.admin {
            true => query,
            false => query.bind(self.id),
        };
        trace!("About to send query");

        let rows: Vec<FileZone> = match query.fetch_all(txn).await {
            Err(error) => {
                error!("Error: {error:?}");
                vec![]
            }
            Ok(rows) => rows.into_iter().map(|row| row.into()).collect(),
        };
        Ok(rows)
    }

    #[instrument(skip(pool))]
    pub async fn get_token(
        pool: &mut Pool<Sqlite>,
        tokenkey: &str,
    ) -> Result<TokenSearchRow, GoatNsError> {
        let mut txn = pool.begin().await?;
        let res: TokenSearchRow = sqlx::query_as(
            "SELECT users.id as userid, users.displayname,  users.username, users.authref, users.email, users.disabled, users.authref, users.admin, tokenhash, tokenkey
            FROM user_tokens, users
            WHERE
                users.disabled=0 AND
                user_tokens.tokenkey=? AND
                user_tokens.userid=users.id")
            .bind(tokenkey)
            .fetch_one(&mut * txn).await?;

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
    pub async fn delete(&self, pool: &SqlitePool) -> Result<(), GoatNsError> {
        // TODO: test ownership delete
        let res = sqlx::query("DELETE FROM ownership WHERE zoneid = ? AND userid = ?")
            .bind(self.zoneid)
            .bind(self.userid)
            .execute(&mut *pool.acquire().await?)
            .await?;
        Ok(())
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete_for_user(self, pool: &SqlitePool) -> Result<User, GoatNsError> {
        // TODO: test user delete
        // TODO: delete all ownership records
        error!("Unimplemented: ZoneOwnership::delete_for_user");
        Err(sqlx::Error::RowNotFound.into())
    }

    // get the thing by the other thing
    pub async fn get_ownership_by_userid<'t>(
        txn: &mut SqliteConnection,
        userid: &i64,
        zoneid: &i64,
    ) -> Result<Option<ZoneOwnership>, GoatNsError> {
        match sqlx::query(
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
        .await
        {
            Ok(val) => Ok(Some(val.into())),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

/// Query the zones table, name_or_id can be the zoneid or the name - if they match then you're bad and you should feel bad.

#[instrument(level = "debug", skip(txn))]
pub async fn get_zone_with_txn(
    txn: &mut SqliteConnection,
    id: Option<i64>,
    name: Option<String>,
) -> Result<Option<FileZone>, GoatNsError> {
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

    let zone_id = match zone.id {
        Some(val) => val,
        None => {
            return Err(GoatNsError::InvalidValue("Zone ID is None".to_string()));
        }
    };

    let result = sqlx::query(
        "SELECT
        id, zoneid, name, ttl, rrtype, rclass, rdata
        FROM records
        WHERE zoneid = ?",
    )
    .bind(zone_id)
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
    type Error = GoatNsError;
    fn try_from(row: SqliteRow) -> Result<InternalResourceRecord, Self::Error> {
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
            id: Some(record_id),
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
) -> Result<Vec<InternalResourceRecord>, GoatNsError> {
    let query = format!(
        "SELECT
        record_id, zoneid, name, rclass, rrtype, rdata, ttl
        FROM {}
        WHERE name = ? AND rrtype = ? AND rclass = ?",
        SQL_VIEW_RECORDS
    );

    let res = sqlx::query(&query)
        .bind(&name)
        .bind(rrtype as u16)
        .bind(rclass as u16)
        .fetch_all(&mut *conn.acquire().await?)
        .await?;

    if res.is_empty() {
        debug!(
            "No results returned for name={} rrtype={} rclass={}",
            name, rrtype as u16, rclass as u16
        );
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
                    error!("Somehow failed to get minimum TTL from query");
                    1
                }
            };

            results
                .to_vec()
                .iter()
                .map(|r| r.clone().set_ttl(min_ttl))
                .collect()
        }
        false => {
            #[cfg(test)]
            println!("not normalizing ttls...");
            results
        }
    };
    Ok(results)
}

impl FileZone {
    pub async fn with_zone_records(self, txn: &mut SqliteConnection) -> Self {
        let records: Vec<FileZoneRecord> = match sqlx::query(
            "SELECT
            id, zoneid, name, ttl, rrtype, rclass, rdata
            FROM records
            WHERE zoneid = ?",
        )
        .bind(self.id)
        .fetch_all(txn)
        .await
        {
            Ok(val) => {
                let res: Vec<FileZoneRecord> = val.into_iter().flat_map(|v| v.try_into()).collect();
                res
            }
            Err(_) => vec![],
        };

        Self { records, ..self }
    }

    /// the records for the zone
    pub async fn get_zone_records(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Vec<FileZoneRecord>, GoatNsError> {
        match self.id {
            Some(id) => {
                let res = sqlx::query(
                    "SELECT
                    id, zoneid, name, ttl, rrtype, rclass, rdata
                    FROM records
                    WHERE zoneid = ?",
                )
                .bind(id)
                .fetch_all(&mut *txn)
                .await?;

                if res.is_empty() {
                    trace!("No results returned for zoneid={:?}", id);
                }

                let results: Vec<FileZoneRecord> = res
                    .into_iter()
                    .filter_map(|r| FileZoneRecord::try_from(r).ok())
                    .collect();

                trace!("results: {results:?}");
                Ok(results)
            }
            None => Err(GoatNsError::InvalidValue("Zone ID is None".to_string())),
        }
    }

    pub async fn get_orphans(pool: &SqlitePool) -> Result<Vec<FileZone>, GoatNsError> {
        let res = sqlx::query(
            "
            SELECT name, userid from zones
            LEFT OUTER JOIN ownership on zones.id = ownership.zoneid
            where userid IS NULL",
        )
        .fetch_all(&mut *pool.acquire().await?)
        .await?;
        let res: Vec<FileZone> = res.into_iter().map(|r| r.into()).collect();
        Ok(res)
    }
}

pub async fn export_zone_json(pool: &SqlitePool, id: i64) -> Result<String, String> {
    let zone = FileZone::get(pool, id)
        .await
        .map_err(|e| format!("{e:?}"))?;
    zone.json()
}

#[async_trait]
pub trait DBEntity: Send {
    const TABLE: &'static str;

    async fn create_table(pool: &SqlitePool) -> Result<(), GoatNsError>;

    /// Get the entity
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Box<Self>, GoatNsError>;
    async fn get_with_txn<'t>(
        _txn: &mut SqliteConnection,
        _id: &i64,
    ) -> Result<Box<Self>, GoatNsError>;
    async fn get_by_name<'t>(
        txn: &mut SqliteConnection,
        name: &str,
    ) -> Result<Option<Box<Self>>, GoatNsError>;
    async fn get_all_by_name<'t>(
        txn: &mut SqliteConnection,
        name: &str,
    ) -> Result<Vec<Box<Self>>, GoatNsError>;
    /// save the entity to the database
    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, GoatNsError>;

    async fn save(&self, pool: &Pool<Sqlite>) -> Result<Box<Self>, GoatNsError>;

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(&self, txn: &mut SqliteConnection)
        -> Result<Box<Self>, GoatNsError>;
    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError>;
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError>;

    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<(), GoatNsError>;
    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, txn: &mut SqliteConnection) -> Result<(), GoatNsError>;

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

    async fn create_table(pool: &SqlitePool) -> Result<(), GoatNsError> {
        let mut tx = pool.begin().await?;

        debug!("Ensuring DB Zones table exists");
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS
            zones (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                rname TEXT NOT NULL,
                serial INTEGER NOT NULL,
                refresh INTEGER NOT NULL,
                retry INTEGER NOT NULL,
                expire INTEGER NOT NULL,
                minimum INTEGER NOT NULL
            )"#,
        )
        .execute(&mut *tx)
        .await?;

        // .execute(tx).await;
        debug!("Ensuring DB Records index exists");
        sqlx::query(
            "CREATE UNIQUE INDEX
            IF NOT EXISTS
            ind_zones
            ON zones (
                id,name
            )",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Get by id
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Box<Self>, GoatNsError> {
        let mut txn = pool.begin().await?;
        Self::get_with_txn(&mut txn, &id).await
    }

    async fn get_with_txn<'t>(
        txn: &mut SqliteConnection,
        id: &i64,
    ) -> Result<Box<Self>, GoatNsError> {
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
        debug!("got a zone: {zone:?}");

        if zone.id.is_none() {
            return Err(sqlx::Error::RowNotFound.into());
        }

        let records = sqlx::query(
            "SELECT
            id, zoneid, name, ttl, rrtype, rclass, rdata
            FROM records
            WHERE zoneid = ?",
        )
        .bind(zone.id)
        .fetch_all(txn)
        .await?;

        zone.records = records
            .into_iter()
            .filter_map(|r| match FileZoneRecord::try_from(r) {
                Ok(val) => Some(val),
                Err(_) => None,
            })
            .collect();
        Ok(Box::new(zone))
    }

    async fn get_by_name<'t>(
        txn: &mut SqliteConnection,
        name: &str,
    ) -> Result<Option<Box<Self>>, GoatNsError> {
        match sqlx::query(&format!("SELECT * from {} where name=?", Self::TABLE))
            .bind(name)
            .fetch_one(&mut *txn)
            .await
        {
            Ok(val) => Ok(Some(Box::new(val.into()))),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
    async fn get_all_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Vec<Box<Self>>, GoatNsError> {
        unimplemented!()
    }
    async fn get_all_user(
        _pool: &Pool<Sqlite>,
        _userid: i64,
    ) -> Result<Vec<Arc<Self>>, GoatNsError> {
        unimplemented!()
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<Box<Self>, GoatNsError> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        // TODO: this needs to include the id
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    #[instrument(skip(txn))]
    async fn save_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        // check the zone exists

        debug!("Getting zone");
        let find_zone = match get_zone_with_txn(txn, None, Some(self.name.clone())).await {
            Ok(val) => {
                trace!("Found existing zone");
                val
            }
            Err(err) => {
                // failed to query the DB
                return Err(err);
            }
        };

        debug!("got zone!");

        let updated_zone: FileZone = match find_zone {
            None => {
                // if it's new, add it
                #[cfg(test)]
                eprintln!("Creating zone {self:?}");
                debug!("Creating zone {self:?}");

                // zone.create
                let serial = self.serial.to_string();
                let refresh = self.refresh.to_string();
                let retry = self.retry.to_string();
                let expire = self.expire.to_string();
                let minimum = self.minimum.to_string();

                sqlx::query(
                    "INSERT INTO zones (id, name, rname, serial, refresh, retry, expire, minimum)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .bind(self.id)
                .bind(&self.name)
                .bind(&self.rname)
                .bind(&serial)
                .bind(&refresh)
                .bind(&retry)
                .bind(&expire)
                .bind(&minimum)
                .execute(&mut *txn)
                .await?;

                #[cfg(not(test))]
                debug!("Insert statement succeeded");
                #[cfg(test)]
                eprintln!("Done creating zone");
                match get_zone_with_txn(txn, None, Some(self.name.clone())).await? {
                    Some(val) => val,
                    None => {
                        return Err(sqlx::Error::RowNotFound.into());
                    }
                }
            }
            Some(ez) => {
                if !self.matching_data(&ez) {
                    // update it if it's wrong

                    debug!("Updating zone");
                    let mut new_zone = self.clone();
                    new_zone.id = ez.id;

                    let updated = new_zone.update_with_txn(txn).await?;

                    debug!("Updated: {:?} record", updated);
                } else {
                    debug!("Zone data is fine")
                }
                match get_zone_with_txn(txn, None, Some(self.name.clone())).await? {
                    Some(val) => val,
                    None => {
                        return Err(sqlx::Error::RowNotFound.into());
                    }
                }
            }
        };
        debug!("Zone after update: {updated_zone:?}");

        // drop all the records
        debug!("Dropping all records for zone {self:?}");
        // debug!("Dropping all records for zone {self:?}");
        sqlx::query("delete from records where zoneid = ?")
            .bind(updated_zone.id)
            .execute(&mut *txn)
            .await?;

        // add the records for the zone
        for mut record in self.records.clone() {
            record.zoneid = updated_zone.id;
            #[cfg(test)]
            eprintln!("Creating new zone record: {record:?}");
            trace!("Creating new zone record: {record:?}");
            if record.name == "@" {
                record.name = "".to_string();
            }
            record.save_with_txn(txn).await?;
        }

        debug!("Done creating zone!");

        let res = Self {
            id: updated_zone.id,
            ..self.to_owned()
        };
        Ok(Box::new(res))
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        error!("Unimplemented: FileZone::create_with_txn");
        Err(sqlx::Error::RowNotFound.into())
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        let _res = sqlx::query(
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
        Ok(Box::new(self.to_owned()))
    }
    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<(), GoatNsError> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(())
    }
    /// Delete the entity from the database, when you're in a transaction.
    ///
    /// This one happens in the order ownership -> records -> zone because at the very least,
    /// if it fails after the ownership thing, then non-admin users can't see the zone
    /// and admins will just have to clean it up manually
    async fn delete_with_txn(&self, txn: &mut SqliteConnection) -> Result<(), GoatNsError> {
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

        Ok(())
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

    async fn create_table(pool: &SqlitePool) -> Result<(), GoatNsError> {
        debug!("Ensuring DB Records table exists");

        let mut tx = pool.begin().await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS
        records (
            id      INTEGER PRIMARY KEY AUTOINCREMENT ,
            zoneid  INTEGER NOT NULL,
            name    TEXT, /* this can be null for apex records */
            ttl     INTEGER,
            rrtype  INTEGER NOT NULL,
            rclass  INTEGER NOT NULL,
            rdata   TEXT NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id)
        )",
        )
        .execute(&mut *tx)
        .await?;
        debug!("Ensuring DB Records index exists");
        sqlx::query(
            "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_records
        ON records (
            id,zoneid,name,rrtype,rclass
        )",
        )
        .execute(&mut *tx)
        .await?;
        debug!("Ensuring DB Records view exists");
        // this view lets us query based on the full name
        sqlx::query(
        &format!("CREATE VIEW IF NOT EXISTS {} ( record_id, zoneid, rrtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zoneid, records.rrtype, records.rclass ,records.rdata,
        CASE
            WHEN records.name is NULL OR length(records.name) == 0 THEN zones.name
            ELSE records.name || '.' || zones.name
        END AS name,
        CASE WHEN records.ttl is NULL then zones.minimum
            WHEN records.ttl > zones.minimum THEN records.ttl
            ELSE records.ttl
        END AS ttl
        from records, zones where records.zoneid = zones.id", SQL_VIEW_RECORDS)
    ).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Get by id
    async fn get(_pool: &Pool<Sqlite>, _id: i64) -> Result<Box<Self>, GoatNsError> {
        error!("Unimplemented: FileZoneRecord::get");
        Err(sqlx::Error::RowNotFound.into())
    }

    async fn get_with_txn<'t>(
        txn: &mut SqliteConnection,
        id: &i64,
    ) -> Result<Box<Self>, GoatNsError> {
        let res = sqlx::query("select * from records where id = ?")
            .bind(id)
            .fetch_one(txn)
            .await?;

        let res = Self::try_from(res)?;

        Ok(Box::new(res))
    }

    async fn get_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Option<Box<Self>>, GoatNsError> {
        unimplemented!();
    }

    async fn get_all_by_name<'t>(
        txn: &mut SqliteConnection,
        name: &str,
    ) -> Result<Vec<Box<Self>>, GoatNsError> {
        let res = sqlx::query(&format!(
            "select * from {} where name = ?",
            SQL_VIEW_RECORDS
        ))
        .bind(name)
        .fetch_all(txn)
        .await?;
        let res = res
            .into_iter()
            .filter_map(|r| match FileZoneRecord::try_from(r) {
                Ok(val) => Some(Box::from(val)),
                Err(err) => {
                    error!("Failed to turn sql row into FileZoneRecord: {:?}", err);
                    None
                }
            })
            .collect();
        Ok(res)
    }

    async fn get_all_user(
        _pool: &Pool<Sqlite>,
        _userid: i64,
    ) -> Result<Vec<Arc<Self>>, GoatNsError> {
        error!("Unimplemented: FileZoneRecord::get_all_user");
        Err(sqlx::Error::RowNotFound.into())
    }

    async fn save(&self, pool: &Pool<Sqlite>) -> Result<Box<Self>, GoatNsError> {
        #[cfg(test)]
        eprintln!("Starting save");
        let mut txn = pool.begin().await?;
        let res = &self.save_with_txn(&mut txn).await?;
        match txn.commit().await {
            Err(err) => {
                eprintln!("Failed to commit transaction: {err:?}");
                return Err(err.into());
            }
            Ok(_) => eprintln!("Successfully saved {self:?} to the db"),
        };
        Ok(res.to_owned())
    }

    async fn save_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        #[cfg(test)]
        eprintln!("Starting save_with_txn for {self:?}");
        trace!("Starting save_with_txn for {self:?}");
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
            .bind(self.id) // TODO id could be a none, which would work out bad
            .bind(self.zoneid) // TODO zoneid could be a none, which would work out bad
            .bind(&record_name)
            .bind(self.ttl)
            .bind(RecordType::from(self.rrtype.clone()))
            .bind(self.class)
            .bind(self.rdata.to_string())
            .fetch_optional(&mut *txn).await?;

        let mut args = SqliteArguments::default();
        args.add(self.zoneid);
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
            None => match self.id {
                Some(id) => sqlx::query(
                    "INSERT INTO records (id, zoneid, name, ttl, rrtype, rclass, rdata)
                                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                                ",
                )
                .bind(id)
                .bind(self.zoneid)
                .bind(self.name.clone())
                .bind(self.ttl)
                .bind(RecordType::from(self.rrtype.clone()))
                .bind(self.class)
                .bind(self.rdata.clone()),
                None => sqlx::query(
                    "INSERT INTO records (zoneid, name, ttl, rrtype, rclass, rdata)
                                        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                                    ",
                )
                .bind(self.zoneid)
                .bind(self.name.clone())
                .bind(self.ttl)
                .bind(RecordType::from(self.rrtype.clone()))
                .bind(self.class)
                .bind(self.rdata.clone()),
            },
        };
        #[cfg(test)]
        println!("Saving record...");
        let res = Self {
            id: Some(query.execute(&mut *txn).await?.last_insert_rowid()),
            ..self.to_owned()
        };

        Ok(Box::new(res))
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        error!("Unimplemented: FileZoneRecord::create_with_txn");
        Err(sqlx::Error::RowNotFound.into())
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        error!("Unimplemented: FileZoneRecord::update_with_txn");
        Err(sqlx::Error::RowNotFound.into())
    }
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<(), GoatNsError> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await
    }

    async fn delete_with_txn(&self, txn: &mut SqliteConnection) -> Result<(), GoatNsError> {
        sqlx::query(format!("DELETE FROM {} WHERE id = ?", Self::TABLE).as_str())
            .bind(self.id)
            .execute(&mut *txn)
            .await?;
        Ok(())
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

    async fn create_table(pool: &SqlitePool) -> Result<(), GoatNsError> {
        let mut tx = pool.begin().await?;

        #[cfg(test)]
        eprintln!("Ensuring DB {} table exists", Self::TABLE);
        debug!("Ensuring DB {} table exists", Self::TABLE);
        sqlx::query(&format!(
            r#"CREATE TABLE IF NOT EXISTS
                {} (
                    id   INTEGER PRIMARY KEY NOT NULL,
                    zoneid INTEGER NOT NULL,
                    userid INTEGER NOT NULL,
                    FOREIGN KEY(zoneid) REFERENCES zones(id),
                    FOREIGN KEY(userid) REFERENCES users(id)
                )"#,
            Self::TABLE
        ))
        .execute(&mut *tx)
        .await?;

        #[cfg(test)]
        eprintln!("Ensuring DB Ownership index exists");
        debug!("Ensuring DB Ownership index exists");
        sqlx::query(
            "CREATE UNIQUE INDEX
                IF NOT EXISTS
                ind_ownership
                ON ownership (
                    zoneid,
                    userid
                )",
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await.map_err(|e| e.into())
    }

    /// Get an ownership record by its id
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Box<Self>, GoatNsError> {
        let mut conn = pool.acquire().await?;

        let res: ZoneOwnership =
            sqlx::query("SELECT id, zoneid, userid from ownership where id = ?")
                .bind(id)
                .fetch_one(&mut *conn)
                .await?
                .into();
        Ok(Box::new(res))
    }

    /// This getter is by zoneid, since it should return less results
    async fn get_with_txn<'t>(
        _txn: &mut SqliteConnection,
        _id: &i64,
    ) -> Result<Box<Self>, GoatNsError> {
        error!("Unimplemented: ZoneOwnership::get_with_txn");
        Err(sqlx::Error::RowNotFound.into())
    }

    async fn get_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Option<Box<Self>>, GoatNsError> {
        // TODO implement ZoneOwnership get_by_name which gets by zone name
        unimplemented!("Not applicable for this!");
    }
    async fn get_all_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Vec<Box<Self>>, GoatNsError> {
        unimplemented!()
    }
    /// Get an ownership record by its id
    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, GoatNsError> {
        let mut conn = pool.acquire().await?;

        let res = sqlx::query("SELECT * from ownership where id = ?")
            .bind(id)
            .fetch_all(&mut *conn)
            .await?;
        let result: Vec<Arc<ZoneOwnership>> = res.into_iter().map(|z| Arc::new(z.into())).collect();
        Ok(result)
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<Box<Self>, GoatNsError> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        let res = sqlx::query(&format!(
            "INSERT INTO {} (zoneid, userid) values ( ?, ? )",
            Self::TABLE
        ))
        .bind(self.zoneid)
        .bind(self.userid)
        .execute(txn)
        .await?;
        // TODO: set the ID to the new ID
        let id: i64 = res.last_insert_rowid();
        let res = Self {
            id: Some(id),
            ..*self
        };
        Ok(Box::new(res))
    }

    /// create new, this just calls save_with_txn
    async fn create_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        self.save_with_txn(txn).await
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        unimplemented!("this should never be updated");
    }
    /// delete the entity from the database
    async fn delete(&self, _pool: &Pool<Sqlite>) -> Result<(), GoatNsError> {
        todo!()
    }

    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, _txn: &mut SqliteConnection) -> Result<(), GoatNsError> {
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

    async fn create_table(pool: &SqlitePool) -> Result<(), GoatNsError> {
        debug!("Ensuring DB Users table exists");
        sqlx::query(&format!(
            r#"CREATE TABLE IF NOT EXISTS
        {} (
            id  INTEGER PRIMARY KEY NOT NULL,
            displayname TEXT NOT NULL,
            username TEXT NOT NULL,
            email TEXT NOT NULL,
            disabled BOOL NOT NULL,
            authref TEXT,
            admin BOOL DEFAULT 0
        )"#,
            Self::TABLE
        ))
        .execute(&mut *pool.acquire().await?)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS
        ind_users_fields
        ON users ( username, email )",
        )
        .execute(&mut *pool.acquire().await?)
        .await?;

        Ok(())
    }
    /// Get an ownership record by its id
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Box<Self>, GoatNsError> {
        let mut conn = pool.acquire().await?;

        let res: User = sqlx::query(&format!(
            "SELECT id, displayname, username, email, disabled from {} where id = ?",
            Self::TABLE
        ))
        .bind(id)
        .fetch_one(&mut *conn)
        .await?
        .into();
        Ok(Box::new(res))
    }
    async fn get_with_txn<'t>(
        _txn: &mut SqliteConnection,
        _id: &i64,
    ) -> Result<Box<Self>, GoatNsError> {
        unimplemented!()
    }
    async fn get_by_name<'t>(
        txn: &mut SqliteConnection,
        name: &str,
    ) -> Result<Option<Box<Self>>, GoatNsError> {
        let res = sqlx::query(&format!(
            "SELECT id, displayname, username, email, disabled, authref, admin from {} where username = ?",
            Self::TABLE
        ))
        .bind(name)
        .fetch_one(&mut *txn)
        .await?;
        let result: Box<Self> = Box::new(res.into());
        Ok(Some(result))
    }
    async fn get_all_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Vec<Box<Self>>, GoatNsError> {
        unimplemented!()
    }
    /// Get an ownership record by its id, which is slightly ironic in this case
    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, GoatNsError> {
        let mut conn = pool.acquire().await?;

        let res = sqlx::query(&format!(
            "SELECT id, zoneid, userid from {} where id = ?",
            Self::TABLE
        ))
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
        let result: Vec<Arc<Self>> = res.into_iter().map(|z| Arc::new(z.into())).collect();
        Ok(result)
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<Box<Self>, GoatNsError> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        let res = sqlx::query(
            "INSERT INTO users
            (id, displayname, username, email, disabled, authref, admin)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(self.id)
        .bind(&self.displayname)
        .bind(&self.username)
        .bind(&self.email)
        .bind(self.disabled)
        .bind(&self.authref)
        .bind(self.admin)
        .execute(txn)
        .await?;
        let res = Self {
            id: Some(res.last_insert_rowid()),
            ..self.to_owned()
        };
        Ok(Box::new(res))
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        todo!();
    }

    async fn update_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        let query = format!("UPDATE {} set displayname = ?, username = ?, email = ?, disabled = ?, authref = ?, admin = ? WHERE id = ?", Self::TABLE);
        sqlx::query(&query)
            .bind(&self.displayname)
            .bind(&self.username)
            .bind(&self.email)
            .bind(self.disabled)
            .bind(&self.authref)
            .bind(self.admin)
            .bind(self.id)
            .execute(txn)
            .await?;
        Ok(Box::new(self.to_owned()))
    }

    /// delete the entity from the database
    async fn delete(&self, _pool: &Pool<Sqlite>) -> Result<(), GoatNsError> {
        todo!()
    }

    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, _txn: &mut SqliteConnection) -> Result<(), GoatNsError> {
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
    ) -> Result<UserAuthToken, GoatNsError> {
        let res = sqlx::query(&format!(
            "select id, issued, expiry, tokenkey, tokenhash, userid from {} where tokenkey = ?",
            Self::TABLE
        ))
        .bind(tokenkey)
        .fetch_one(&mut *pool.acquire().await?)
        .await?;
        res.try_into()
    }

    pub async fn cleanup(pool: &SqlitePool) -> Result<(), GoatNsError> {
        let current_time = Utc::now();
        debug!(
            "Starting cleanup of {} table for sessions expiring before {}",
            Self::TABLE,
            current_time.to_rfc3339()
        );

        match sqlx::query(&format!(
            "DELETE FROM {} where expiry NOT NULL and expiry < ?",
            Self::TABLE
        ))
        .bind(current_time.timestamp())
        .execute(&mut *pool.acquire().await?)
        .await
        {
            Ok(res) => {
                info!(
                    "Cleanup of {} table complete, {} rows deleted.",
                    Self::TABLE,
                    res.rows_affected()
                );
                Ok(())
            }
            Err(error) => {
                error!(
                    "Failed to complete cleanup of {} table: {error:?}",
                    Self::TABLE
                );
                Err(error.into())
            }
        }
    }
}

#[async_trait]
impl DBEntity for UserAuthToken {
    const TABLE: &'static str = "user_tokens";

    async fn create_table(pool: &SqlitePool) -> Result<(), GoatNsError> {
        let mut conn = pool.acquire().await?;
        debug!("Ensuring DB {} table exists", Self::TABLE);

        match sqlx::query(&format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
            Self::TABLE
        ))
        .fetch_optional(&mut *conn)
        .await?
        {
            None => {
                debug!("Creating {} table", Self::TABLE);
                sqlx::query(&format!(
                    r#"CREATE TABLE IF NOT EXISTS
                    {} (
                        id INTEGER PRIMARY KEY NOT NULL,
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
                .execute(&mut *conn)
                .await?;
            }
            Some(_) => {
                debug!("Updating the table");
                // get the columns in the table
                let res = sqlx::query(&format!("PRAGMA table_info({})", Self::TABLE))
                    .fetch_all(&mut *conn)
                    .await?;

                let mut found_name = false;
                let mut found_tokenkey = false;
                for row in res.iter() {
                    let rowname: &str = row.get("name");
                    if rowname == "name" {
                        debug!("Found the name column in the {} table", Self::TABLE);
                        found_name = true;
                    }

                    let rowname: &str = row.get("name");
                    if rowname == "tokenkey" {
                        debug!("Found the tokenkey column in the {} table", Self::TABLE);
                        found_tokenkey = true;
                    }
                }

                if !found_name {
                    info!("Adding the name column to the {} table", Self::TABLE);
                    sqlx::query(&format!(
                        "ALTER TABLE \"{}\" ADD COLUMN name TEXT NOT NULL DEFAULT \"Token Name\"",
                        Self::TABLE
                    ))
                    .execute(&mut *conn)
                    .await?;
                }
                if !found_tokenkey {
                    info!("Adding the tokenkey column to the {} table, this will drop the contents of the API tokens table, because of the format change.", Self::TABLE);

                    match dialoguer::Confirm::new()
                        .with_prompt("Please confirm that you want to take this action")
                        .interact()
                    {
                        Ok(value) => {
                            if !value {
                                return Err(sqlx::Error::Protocol("Cancelled".to_string()).into());
                                // TODO: replace these with proper errors
                            }
                        }
                        Err(error) => {
                            error!("Cancelled! {error:?}");
                            return Err(sqlx::Error::Protocol("Cancelled".to_string()).into());
                            // TODO: replace these with proper errors
                        }
                    };
                    sqlx::query(&format!("DELETE FROM {}", Self::TABLE))
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query(&format!(
                        "ALTER TABLE \"{}\" ADD COLUMN tokenkey TEXT NOT NULL DEFAULT \"old_tokenkey\"",
                        Self::TABLE
                    ))
                    .execute(&mut *conn)
                    .await?;
                }
            }
        };

        match sqlx::query(&format!("DROP INDEX ind_{}_fields", Self::TABLE))
            .execute(&mut *conn)
            .await
        {
            Ok(_) => trace!(
                "Didn't find  ind_{}_fields index, no action required",
                Self::TABLE
            ),
            Err(err) => match err {
                sqlx::Error::Database(ref zzz) => {
                    if zzz.message() != "no such index: ind_user_tokens_fields" {
                        error!("Database Error: {:?}", zzz);
                        return Err(err.into());
                    }
                }
                _ => {
                    error!("{err:?}");
                    return Err(err.into());
                }
            },
        };

        sqlx::query(&format!(
            "CREATE UNIQUE INDEX IF NOT EXISTS
        ind_{0}_findit
        ON {0} ( userid, tokenkey, tokenhash )",
            Self::TABLE
        ))
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    /// Get the entity
    async fn get(pool: &Pool<Sqlite>, id: i64) -> Result<Box<Self>, GoatNsError> {
        let res: Self = sqlx::query(&format!("SELECT * from {} where id = ?", Self::TABLE))
            .bind(id)
            .fetch_one(pool)
            .await?
            .try_into()?;

        Ok(Box::new(res))
    }

    async fn get_with_txn<'t>(
        _txn: &mut SqliteConnection,
        _id: &i64,
    ) -> Result<Box<Self>, GoatNsError> {
        unimplemented!()
    }
    // TODO: maybe get by name gets it by the username?
    async fn get_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Option<Box<Self>>, GoatNsError> {
        unimplemented!()
    }
    async fn get_all_by_name<'t>(
        _txn: &mut SqliteConnection,
        _name: &str,
    ) -> Result<Vec<Box<Self>>, GoatNsError> {
        unimplemented!()
    }

    async fn get_all_user(pool: &Pool<Sqlite>, id: i64) -> Result<Vec<Arc<Self>>, GoatNsError> {
        let res = sqlx::query(&format!("SELECT * from {} where userid = ?", Self::TABLE))
            .bind(id)
            .fetch_all(pool)
            .await?;
        let res: Vec<Arc<UserAuthToken>> = res
            .into_iter()
            .filter_map(|r| match UserAuthToken::try_from(r) {
                Ok(r) => Some(Arc::new(r)),
                Err(e) => {
                    error!("Failed to convert row to UserAuthToken: {e:?}");
                    None
                }
            })
            .collect();
        Ok(res)
    }

    /// save the entity to the database
    async fn save(&self, pool: &Pool<Sqlite>) -> Result<Box<Self>, GoatNsError> {
        let mut txn = pool.begin().await?;
        let res = self.save_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(res)
    }

    /// save the entity to the database, but you're in a transaction
    async fn save_with_txn<'t>(
        &self,
        txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        let expiry = self.expiry.map(|v| v.timestamp());
        let issued = self.issued.timestamp();

        let res = sqlx::query(&format!(
            "INSERT INTO {} (id, name, issued, expiry, userid, tokenkey, tokenhash) VALUES (?, ?, ?, ?, ?, ?, ?)",
            Self::TABLE
        ))
        .bind(self.id)
        .bind(&self.name)
        .bind(issued)
        .bind(expiry)
        .bind(self.userid)
        .bind(&self.tokenkey)
        .bind(&self.tokenhash)
        .execute(txn)
        .await?;

        let res = Self {
            id: Some(res.last_insert_rowid()),
            ..self.to_owned()
        };

        Ok(Box::new(res))
    }

    /// create from scratch
    async fn create_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        todo!();
    }
    /// create from scratch
    async fn update_with_txn<'t>(
        &self,
        _txn: &mut SqliteConnection,
    ) -> Result<Box<Self>, GoatNsError> {
        todo!();
    }

    /// delete the entity from the database
    async fn delete(&self, pool: &Pool<Sqlite>) -> Result<(), GoatNsError> {
        let mut txn = pool.begin().await?;
        self.delete_with_txn(&mut txn).await?;
        txn.commit().await?;
        Ok(())
    }
    /// delete the entity from the database, but you're in a transaction
    async fn delete_with_txn(&self, txn: &mut SqliteConnection) -> Result<(), GoatNsError> {
        sqlx::query(&format!("DELETE FROM {} where id = ?", &Self::TABLE))
            .bind(self.id)
            .execute(txn)
            .await?;
        Ok(())
    }

    fn json(&self) -> Result<String, String>
    where
        Self: Serialize,
    {
        serde_json::to_string_pretty(&self).map_err(|e| e.to_string())
    }
}

impl TryFrom<SqliteRow> for UserAuthToken {
    type Error = GoatNsError;
    fn try_from(input: SqliteRow) -> Result<Self, Self::Error> {
        let expiry: Option<String> = input.get("expiry");
        let expiry: Option<DateTime<Utc>> = match expiry {
            None => None,
            Some(val) => {
                let expiry = chrono::NaiveDateTime::parse_from_str(&val, "%s")?;
                let expiry: DateTime<Utc> = chrono::TimeZone::from_utc_datetime(&Utc, &expiry);
                Some(expiry)
            }
        };

        let issued: String = input.get("issued");

        let issued = chrono::NaiveDateTime::parse_from_str(&issued, "%s")?;
        let issued: DateTime<Utc> = chrono::TimeZone::from_utc_datetime(&Utc, &issued);

        Ok(Self {
            id: input.get("id"),
            name: input.get("name"),
            issued,
            expiry,
            userid: input.get("userid"),
            tokenkey: input.get("tokenkey"),
            tokenhash: input.get("tokenhash"),
        })
    }
}

/// Run this periodically to clean up expired DB things
pub async fn cron_db_cleanup(pool: Pool<Sqlite>, period: Duration, max_iter: Option<usize>) {
    let mut interval = time::interval(period);
    let mut iterations = 0;
    loop {
        interval.tick().await;

        if let Err(error) = UserAuthToken::cleanup(&pool).await {
            error!("Failed to clean up UserAuthToken objects in DB cron: {error:?}");
        }
        if let Some(max_iter) = max_iter {
            iterations += 1;
            if iterations >= max_iter {
                break;
            }
        }
    }
}

pub async fn get_zones_with_txn(
    txn: &mut SqliteConnection,
    lim: i64,
    offset: i64,
) -> Result<Vec<FileZone>, GoatNsError> {
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
        .into_iter()
        .map(|row| {
            #[cfg(test)]
            eprintln!("Building FileZone");
            row.into()
        })
        .collect();
    Ok(rows)
}

impl TryFrom<SqliteRow> for FileZoneRecord {
    type Error = GoatNsError;
    fn try_from(row: SqliteRow) -> Result<Self, Self::Error> {
        let name: String = row.get("name");
        let rrtype: i32 = row.get("rrtype");
        let rrtype = RecordType::from(&(rrtype as u16));
        let class: u16 = row.get("rclass");
        let rdata: String = row.get("rdata");
        let ttl: u32 = row.get("ttl");

        if let RecordType::ANY = rrtype {
            return Err(GoatNsError::RFC8482);
        }

        Ok(Self {
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

pub async fn get_all_fzr_by_name<'t>(
    txn: &mut SqliteConnection,
    name: &str,
    rrtype: u16,
) -> Result<Vec<FileZoneRecord>, GoatNsError> {
    let res = sqlx::query(&format!(
        "select *, record_id as id from {} where name = ? AND rrtype = ?",
        SQL_VIEW_RECORDS
    ))
    .bind(name)
    .bind(rrtype)
    .fetch_all(txn)
    .await?;

    let res = res.into_iter().filter_map(|r| r.try_into().ok()).collect();
    Ok(res)
}

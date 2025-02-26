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
use sqlx::sqlite::{SqliteConnectOptions, SqliteRow};
use sqlx::{ConnectOptions, FromRow, Pool, Row, Sqlite, SqliteConnection, SqlitePool};
use tokio::time;
use tracing::{debug, error, info, instrument, trace};
use zoneownership::ZoneOwnership;

pub(crate) mod entities;
#[cfg(test)]
pub mod test;

pub(crate) mod filezone;
pub(crate) mod filezonerecord;
pub(crate) mod prelude;
pub(crate) mod user;
pub(crate) mod zoneownership;
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
                name: row.get("name"),
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

pub async fn get_all_fzr_by_name(
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

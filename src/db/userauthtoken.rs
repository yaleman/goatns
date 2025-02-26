use super::prelude::*;
use chrono::{DateTime, Utc};

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

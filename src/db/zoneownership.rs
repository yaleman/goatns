use super::prelude::*;

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
    pub async fn get_ownership_by_userid(
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

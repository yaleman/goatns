use super::prelude::*;

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

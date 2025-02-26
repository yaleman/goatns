use crate::{
    db::get_zone_with_txn,
    zones::{FileZone, FileZoneRecord},
};

use super::prelude::*;

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

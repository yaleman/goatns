use sqlx::sqlite::SqliteArguments;
use sqlx::Arguments;

use crate::{db::SQL_VIEW_RECORDS, enums::RecordType, zones::FileZoneRecord};

use super::prelude::*;

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

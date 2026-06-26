use async_trait::async_trait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP VIEW IF EXISTS records_merged;")
            .await?;

        db.execute_unprepared(
            r#"CREATE VIEW records_merged ( record_id, zoneid, rrtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zoneid, records.rrtype, records.rclass, records.rdata,
        CASE
            WHEN records.name is NULL OR length(records.name) == 0 OR records.name = '@' THEN zones.name
            ELSE records.name || '.' || zones.name
        END AS name,
        CASE WHEN records.ttl is NULL then zones.minimum
            WHEN records.ttl > zones.minimum THEN records.ttl
            ELSE records.ttl
        END AS ttl
        from records, zones where records.zoneid = zones.id;"#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

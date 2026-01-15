use async_trait::async_trait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Zones::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Zones::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Zones::Name).string().not_null())
                    .col(ColumnDef::new(Zones::Rname).string().not_null())
                    .col(ColumnDef::new(Zones::Serial).big_integer().not_null())
                    .col(ColumnDef::new(Zones::Refresh).big_integer().not_null())
                    .col(ColumnDef::new(Zones::Retry).big_integer().not_null())
                    .col(ColumnDef::new(Zones::Expire).big_integer().not_null())
                    .col(ColumnDef::new(Zones::Minimum).big_integer().not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Records::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Records::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Records::Zoneid).uuid().not_null())
                    .col(ColumnDef::new(Records::Name).string().not_null())
                    .col(ColumnDef::new(Records::Ttl).integer())
                    .col(ColumnDef::new(Records::Rrtype).unsigned().not_null())
                    .col(ColumnDef::new(Records::Rclass).unsigned().not_null())
                    .col(ColumnDef::new(Records::Rdata).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-records-zoneid")
                            .from(Records::Table, Records::Zoneid)
                            .to(Zones::Table, Zones::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Ownership::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Ownership::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Ownership::Zoneid).uuid().not_null())
                    .col(ColumnDef::new(Ownership::Userid).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-ownership-zoneid")
                            .from(Ownership::Table, Ownership::Zoneid)
                            .to(Zones::Table, Zones::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-ownership-userid")
                            .from(Ownership::Table, Ownership::Userid)
                            .to(
                                crate::db::migrations::m_20260115_users_table::Users::Table,
                                crate::db::migrations::m_20260115_users_table::Users::Id,
                            )
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // create the records_merged view
        manager.get_connection().execute_unprepared(
            r#"CREATE VIEW IF NOT EXISTS records_merged ( record_id, zoneid, rrtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zoneid, records.rrtype, records.rclass ,records.rdata,
        CASE
            WHEN records.name is NULL OR length(records.name) == 0 THEN zones.name
            ELSE records.name || '.' || zones.name
        END AS name,
        CASE WHEN records.ttl is NULL then zones.minimum
            WHEN records.ttl > zones.minimum THEN records.ttl
            ELSE records.ttl
        END AS ttl
        from records, zones where records.zoneid = zones.id;"#,
        ).await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(DeriveIden)]
pub(crate) enum Zones {
    Table,
    Id,
    Name,
    Rname,
    Serial,
    Refresh,
    Retry,
    Expire,
    Minimum,
}

#[derive(DeriveIden)]
pub(crate) enum Records {
    Table,
    Id,
    Zoneid,
    Name,
    Ttl,
    Rrtype,
    Rclass,
    Rdata,
}

#[derive(DeriveIden)]
enum Ownership {
    Table,
    Id,
    Zoneid,
    Userid,
}

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        if !sqlite_table_exists(db, "zones").await? {
            return Ok(());
        }

        let zones_id_integer = column_is_integer(db, "zones", "id").await?;
        let has_text_uuids = any_text_uuid_values(db).await?;
        if !zones_id_integer && !has_text_uuids {
            return Ok(());
        }

        db.execute_unprepared("PRAGMA foreign_keys=OFF;").await?;
        db.execute_unprepared("DROP VIEW IF EXISTS records_merged;")
            .await?;

        if !zones_id_integer && has_text_uuids {
            convert_text_uuid_column(db, "zones", "id").await?;
            if sqlite_table_exists(db, "records").await? {
                convert_text_uuid_column(db, "records", "id").await?;
                convert_text_uuid_column(db, "records", "zoneid").await?;
            }
            if sqlite_table_exists(db, "ownership").await? {
                convert_text_uuid_column(db, "ownership", "id").await?;
                convert_text_uuid_column(db, "ownership", "zoneid").await?;
                convert_text_uuid_column(db, "ownership", "userid").await?;
            }
        }

        if zones_id_integer {
            db.execute_unprepared("DROP TABLE IF EXISTS zone_id_map;")
                .await?;
            db.execute_unprepared(
                "CREATE TABLE zone_id_map (old_id_text TEXT PRIMARY KEY, new_id BLOB NOT NULL);",
            )
            .await?;

            let rows = db
                .query_all(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT CAST(id AS TEXT) as id_text FROM zones;",
                ))
                .await?;
            for row in rows {
                let old_id: String = row.try_get("", "id_text")?;
                let new_id_bytes = Uuid::now_v7().as_bytes().to_vec();
                let stmt = Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO zone_id_map (old_id_text, new_id) VALUES (?, ?);",
                    vec![old_id.into(), Value::Bytes(Some(Box::new(new_id_bytes)))],
                );
                db.execute(stmt).await?;
            }

            db.execute_unprepared("DROP TABLE IF EXISTS zones_new;")
                .await?;
            manager
                .create_table(
                    Table::create()
                        .table(ZonesNew::Table)
                        .col(ColumnDef::new(ZonesNew::Id).uuid().not_null().primary_key())
                        .col(ColumnDef::new(ZonesNew::Name).string().not_null())
                        .col(ColumnDef::new(ZonesNew::Rname).string().not_null())
                        .col(ColumnDef::new(ZonesNew::Serial).big_integer().not_null())
                        .col(ColumnDef::new(ZonesNew::Refresh).big_integer().not_null())
                        .col(ColumnDef::new(ZonesNew::Retry).big_integer().not_null())
                        .col(ColumnDef::new(ZonesNew::Expire).big_integer().not_null())
                        .col(ColumnDef::new(ZonesNew::Minimum).big_integer().not_null())
                        .to_owned(),
                )
                .await?;
            db.execute_unprepared(
                "INSERT INTO zones_new (id, name, rname, serial, refresh, retry, expire, minimum)
                SELECT zone_id_map.new_id, zones.name, zones.rname, zones.serial,
                       zones.refresh, zones.retry, zones.expire, zones.minimum
                FROM zones
                JOIN zone_id_map ON zone_id_map.old_id_text = CAST(zones.id AS TEXT);",
            )
            .await?;

            if sqlite_table_exists(db, "records").await? {
                db.execute_unprepared("DROP TABLE IF EXISTS record_id_map;")
                    .await?;
                db.execute_unprepared(
                    "CREATE TABLE record_id_map (old_id_text TEXT PRIMARY KEY, new_id BLOB NOT NULL);",
                )
                .await?;
                let rows = db
                    .query_all(Statement::from_string(
                        DbBackend::Sqlite,
                        "SELECT CAST(id AS TEXT) as id_text FROM records;",
                    ))
                    .await?;
                for row in rows {
                    let old_id: String = row.try_get("", "id_text")?;
                    let new_id_bytes = Uuid::now_v7().as_bytes().to_vec();
                    let stmt = Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "INSERT INTO record_id_map (old_id_text, new_id) VALUES (?, ?);",
                        vec![old_id.into(), Value::Bytes(Some(Box::new(new_id_bytes)))],
                    );
                    db.execute(stmt).await?;
                }

                db.execute_unprepared("DROP TABLE IF EXISTS records_new;")
                    .await?;
                manager
                    .create_table(
                        Table::create()
                            .table(RecordsNew::Table)
                            .col(
                                ColumnDef::new(RecordsNew::Id)
                                    .uuid()
                                    .not_null()
                                    .primary_key(),
                            )
                            .col(ColumnDef::new(RecordsNew::Zoneid).uuid().not_null())
                            .col(ColumnDef::new(RecordsNew::Name).string().not_null())
                            .col(ColumnDef::new(RecordsNew::Ttl).integer())
                            .col(ColumnDef::new(RecordsNew::Rrtype).unsigned().not_null())
                            .col(ColumnDef::new(RecordsNew::Rclass).unsigned().not_null())
                            .col(ColumnDef::new(RecordsNew::Rdata).string().not_null())
                            .foreign_key(
                                ForeignKey::create()
                                    .name("fk-records-zoneid")
                                    .from(RecordsNew::Table, RecordsNew::Zoneid)
                                    .to(Zones::Table, Zones::Id)
                                    .on_update(ForeignKeyAction::Cascade)
                                    .on_delete(ForeignKeyAction::Cascade),
                            )
                            .to_owned(),
                    )
                    .await?;
                db.execute_unprepared(
                "INSERT INTO records_new (id, zoneid, name, ttl, rrtype, rclass, rdata)
                    SELECT record_id_map.new_id, zone_id_map.new_id, records.name, records.ttl,
                           records.rrtype, records.rclass, records.rdata
                    FROM records
                    JOIN zone_id_map ON zone_id_map.old_id_text = CAST(records.zoneid AS TEXT)
                    JOIN record_id_map ON record_id_map.old_id_text = CAST(records.id AS TEXT);",
                )
                .await?;
            }

            if sqlite_table_exists(db, "ownership").await? {
                db.execute_unprepared("DROP TABLE IF EXISTS ownership_id_map;")
                    .await?;
                db.execute_unprepared(
                    "CREATE TABLE ownership_id_map (old_id_text TEXT PRIMARY KEY, new_id BLOB NOT NULL);",
                )
                .await?;
                let rows = db
                    .query_all(Statement::from_string(
                        DbBackend::Sqlite,
                        "SELECT CAST(id AS TEXT) as id_text FROM ownership;",
                    ))
                    .await?;
                for row in rows {
                    let old_id: String = row.try_get("", "id_text")?;
                    let new_id_bytes = Uuid::now_v7().as_bytes().to_vec();
                    let stmt = Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "INSERT INTO ownership_id_map (old_id_text, new_id) VALUES (?, ?);",
                        vec![old_id.into(), Value::Bytes(Some(Box::new(new_id_bytes)))],
                    );
                    db.execute(stmt).await?;
                }

                db.execute_unprepared("DROP TABLE IF EXISTS ownership_new;")
                    .await?;
                manager
                    .create_table(
                        Table::create()
                            .table(OwnershipNew::Table)
                            .col(
                                ColumnDef::new(OwnershipNew::Id)
                                    .uuid()
                                    .not_null()
                                    .primary_key(),
                            )
                            .col(ColumnDef::new(OwnershipNew::Zoneid).uuid().not_null())
                            .col(ColumnDef::new(OwnershipNew::Userid).uuid().not_null())
                            .foreign_key(
                                ForeignKey::create()
                                    .name("fk-ownership-zoneid")
                                    .from(OwnershipNew::Table, OwnershipNew::Zoneid)
                                    .to(Zones::Table, Zones::Id)
                                    .on_update(ForeignKeyAction::Cascade)
                                    .on_delete(ForeignKeyAction::Cascade),
                            )
                            .foreign_key(
                                ForeignKey::create()
                                    .name("fk-ownership-userid")
                                    .from(OwnershipNew::Table, OwnershipNew::Userid)
                                    .to(Users::Table, Users::Id)
                                    .on_update(ForeignKeyAction::Cascade)
                                    .on_delete(ForeignKeyAction::Cascade),
                            )
                            .to_owned(),
                    )
                    .await?;
                db.execute_unprepared(
                "INSERT INTO ownership_new (id, zoneid, userid)
                    SELECT ownership_id_map.new_id, zone_id_map.new_id, ownership.userid
                    FROM ownership
                    JOIN zone_id_map ON zone_id_map.old_id_text = CAST(ownership.zoneid AS TEXT)
                    JOIN ownership_id_map ON ownership_id_map.old_id_text = CAST(ownership.id AS TEXT);",
                )
                .await?;
            }

            if sqlite_table_exists(db, "records").await? {
                db.execute_unprepared("DROP TABLE records;").await?;
                db.execute_unprepared("ALTER TABLE records_new RENAME TO records;")
                    .await?;
                db.execute_unprepared("DROP TABLE record_id_map;").await?;
            }
            if sqlite_table_exists(db, "ownership").await? {
                db.execute_unprepared("DROP TABLE ownership;").await?;
                db.execute_unprepared("ALTER TABLE ownership_new RENAME TO ownership;")
                    .await?;
                db.execute_unprepared("DROP TABLE ownership_id_map;")
                    .await?;
            }
            db.execute_unprepared("DROP TABLE zones;").await?;
            db.execute_unprepared("ALTER TABLE zones_new RENAME TO zones;")
                .await?;
            db.execute_unprepared("DROP TABLE zone_id_map;").await?;
        }

        recreate_records_view(db).await?;
        db.execute_unprepared("PRAGMA foreign_keys=ON;").await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

async fn sqlite_table_exists<C: ConnectionTrait>(db: &C, name: &str) -> Result<bool, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?;",
        vec![name.into()],
    );
    Ok(db.query_one(stmt).await?.is_some())
}

async fn column_is_integer<C: ConnectionTrait>(
    db: &C,
    table: &str,
    column: &str,
) -> Result<bool, DbErr> {
    let pragma_sql = format!("PRAGMA table_info({table});");
    let rows = db
        .query_all(Statement::from_string(DbBackend::Sqlite, pragma_sql))
        .await?;
    for row in rows {
        let column_name: String = row.try_get("", "name")?;
        if column_name == column {
            let column_type: String = row.try_get("", "type")?;
            return Ok(column_type.to_ascii_lowercase().contains("int"));
        }
    }
    Ok(false)
}

async fn any_text_uuid_values<C: ConnectionTrait>(db: &C) -> Result<bool, DbErr> {
    let checks = [
        ("zones", "id"),
        ("records", "id"),
        ("records", "zoneid"),
        ("ownership", "id"),
        ("ownership", "zoneid"),
        ("ownership", "userid"),
    ];
    for (table, column) in checks {
        if sqlite_table_exists(db, table).await? {
            let values = text_values(db, table, column).await?;
            if values.iter().any(|value| looks_like_uuid(value)) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

async fn convert_text_uuid_column<C: ConnectionTrait>(
    db: &C,
    table: &str,
    column: &str,
) -> Result<(), DbErr> {
    let values = text_values(db, table, column).await?;
    let uuid_values: Vec<String> = values
        .into_iter()
        .filter(|value| looks_like_uuid(value))
        .collect();
    if uuid_values.is_empty() {
        return Ok(());
    }

    db.execute_unprepared("DROP TABLE IF EXISTS uuid_text_map;")
        .await?;
    db.execute_unprepared(
        "CREATE TABLE uuid_text_map (old_id TEXT PRIMARY KEY, new_id BLOB NOT NULL);",
    )
    .await?;

    for old_id in uuid_values {
        let parsed = Uuid::parse_str(&old_id).map_err(|error| {
            DbErr::Custom(format!(
                "failed to parse {table}.{column} '{old_id}' as uuid: {error}"
            ))
        })?;
        let new_id_bytes = parsed.as_bytes().to_vec();
        let stmt = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO uuid_text_map (old_id, new_id) VALUES (?, ?);",
            vec![old_id.into(), Value::Bytes(Some(Box::new(new_id_bytes)))],
        );
        db.execute(stmt).await?;
    }

    let update_sql = format!(
        "UPDATE {table}
        SET {column} = (SELECT new_id FROM uuid_text_map WHERE old_id = {table}.{column})
        WHERE typeof({column}) = 'text' AND {column} IN (SELECT old_id FROM uuid_text_map);"
    );
    db.execute_unprepared(&update_sql).await?;
    db.execute_unprepared("DROP TABLE uuid_text_map;").await?;

    Ok(())
}

async fn recreate_records_view<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    if !sqlite_table_exists(db, "records").await? || !sqlite_table_exists(db, "zones").await? {
        return Ok(());
    }

    db.execute_unprepared(
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
    )
    .await?;

    Ok(())
}

async fn text_values<C: ConnectionTrait>(
    db: &C,
    table: &str,
    column: &str,
) -> Result<Vec<String>, DbErr> {
    let select_sql =
        format!("SELECT DISTINCT {column} as id FROM {table} WHERE typeof({column}) = 'text';");
    let rows = db
        .query_all(Statement::from_string(DbBackend::Sqlite, select_sql))
        .await?;
    let mut values = Vec::new();
    for row in rows {
        let value: String = row.try_get("", "id")?;
        values.push(value);
    }
    Ok(values)
}

fn looks_like_uuid(value: &str) -> bool {
    let len = value.len();
    let is_hex = |ch: char| ch.is_ascii_hexdigit();
    if len == 36 {
        let positions = [8, 13, 18, 23];
        if positions.iter().all(|&idx| value.as_bytes()[idx] == b'-') {
            return value.chars().filter(|ch| *ch != '-').all(is_hex);
        }
    } else if len == 32 {
        return value.chars().all(is_hex);
    }
    false
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Zones {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum ZonesNew {
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
enum RecordsNew {
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
enum OwnershipNew {
    Table,
    Id,
    Zoneid,
    Userid,
}

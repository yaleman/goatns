use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let is_integer_id = users_id_is_integer(db).await?;
        let has_text_id = users_id_has_text_values(db).await?;
        if !is_integer_id && !has_text_id {
            return Ok(());
        }

        db.execute_unprepared("PRAGMA foreign_keys=OFF;").await?;

        if !is_integer_id && has_text_id {
            db.execute_unprepared("DROP TABLE IF EXISTS user_id_map;")
                .await?;
            db.execute_unprepared(
                "CREATE TABLE user_id_map (old_id TEXT PRIMARY KEY, new_id BLOB NOT NULL);",
            )
            .await?;

            let rows = db
                .query_all(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT id FROM users WHERE typeof(id) = 'text';",
                ))
                .await?;
            for row in rows {
                let old_id: String = row.try_get("", "id")?;
                let parsed = Uuid::parse_str(&old_id).map_err(|error| {
                    DbErr::Custom(format!(
                        "failed to parse users.id '{old_id}' as uuid: {error}"
                    ))
                })?;
                let new_id_bytes = parsed.as_bytes().to_vec();
                let stmt = Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO user_id_map (old_id, new_id) VALUES (?, ?);",
                    vec![old_id.into(), Value::Bytes(Some(Box::new(new_id_bytes)))],
                );
                db.execute(stmt).await?;
            }

            db.execute_unprepared(
                "UPDATE users
                SET id = (SELECT new_id FROM user_id_map WHERE old_id = users.id)
                WHERE id IN (SELECT old_id FROM user_id_map);",
            )
            .await?;

            if sqlite_table_exists(db, "user_tokens").await? {
                db.execute_unprepared(
                    "UPDATE user_tokens
                    SET userid = (SELECT new_id FROM user_id_map WHERE old_id = user_tokens.userid)
                    WHERE userid IN (SELECT old_id FROM user_id_map);",
                )
                .await?;
            }

            if sqlite_table_exists(db, "ownership").await? {
                db.execute_unprepared(
                    "UPDATE ownership
                    SET userid = (SELECT new_id FROM user_id_map WHERE old_id = ownership.userid)
                    WHERE userid IN (SELECT old_id FROM user_id_map);",
                )
                .await?;
            }

            db.execute_unprepared("DROP TABLE user_id_map;").await?;
            db.execute_unprepared("PRAGMA foreign_keys=ON;").await?;
            return Ok(());
        }

        db.execute_unprepared("DROP TABLE IF EXISTS user_id_map;")
            .await?;
        db.execute_unprepared(
            "CREATE TABLE user_id_map (old_id INTEGER PRIMARY KEY, new_id BLOB NOT NULL);",
        )
        .await?;

        let rows = db
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id FROM users;",
            ))
            .await?;
        for row in rows {
            let old_id: i64 = row.try_get("", "id")?;
            let new_id = Uuid::now_v7();
            let new_id_bytes = new_id.as_bytes().to_vec();
            let stmt = Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO user_id_map (old_id, new_id) VALUES (?, ?);",
                vec![
                    old_id.into(),
                    Value::Bytes(Some(Box::new(new_id_bytes))),
                ],
            );
            db.execute(stmt).await?;
        }

        db.execute_unprepared("DROP TABLE IF EXISTS users_new;")
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(UsersNew::Table)
                    .col(ColumnDef::new(UsersNew::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(UsersNew::Displayname).string().not_null())
                    .col(ColumnDef::new(UsersNew::Username).string().not_null())
                    .col(ColumnDef::new(UsersNew::Email).string().not_null())
                    .col(ColumnDef::new(UsersNew::Disabled).boolean().not_null())
                    .col(ColumnDef::new(UsersNew::Authref).string())
                    .col(ColumnDef::new(UsersNew::Admin).boolean().not_null())
                    .to_owned(),
            )
            .await?;
        db.execute_unprepared(
            "INSERT INTO users_new (id, displayname, username, email, disabled, authref, admin)
            SELECT user_id_map.new_id, users.displayname, users.username, users.email,
                   users.disabled, users.authref, users.admin
            FROM users
            JOIN user_id_map ON user_id_map.old_id = users.id;",
        )
        .await?;

        let has_user_tokens = sqlite_table_exists(db, "user_tokens").await?;
        if has_user_tokens {
            let token_columns = user_tokens_columns(db).await?;
            db.execute_unprepared("DROP TABLE IF EXISTS user_tokens_new;")
                .await?;
            manager
                .create_table(
                    Table::create()
                        .table(UserTokensNew::Table)
                        .col(
                            ColumnDef::new(UserTokensNew::Id)
                                .big_integer()
                                .not_null()
                                .primary_key()
                                .auto_increment(),
                        )
                        .col(ColumnDef::new(UserTokensNew::Name).string().not_null())
                        .col(ColumnDef::new(UserTokensNew::Issued).date_time().not_null())
                        .col(ColumnDef::new(UserTokensNew::Expiry).date_time())
                        .col(ColumnDef::new(UserTokensNew::Key).string().not_null())
                        .col(ColumnDef::new(UserTokensNew::Hash).string().not_null())
                        .col(ColumnDef::new(UserTokensNew::Userid).uuid().not_null())
                        .foreign_key(
                            ForeignKey::create()
                                .name("fk-user_tokens-userid")
                                .from(UserTokensNew::Table, UserTokensNew::Userid)
                                .to(Users::Table, Users::Id)
                                .on_update(ForeignKeyAction::Cascade)
                                .on_delete(ForeignKeyAction::Cascade),
                        )
                        .to_owned(),
                )
                .await?;
            let insert_sql = format!(
                "INSERT INTO user_tokens_new (id, name, issued, expiry, key, hash, userid)
                SELECT user_tokens.id, user_tokens.name, user_tokens.issued, user_tokens.expiry,
                       user_tokens.{key_col}, user_tokens.{hash_col}, user_id_map.new_id
                FROM user_tokens
                JOIN user_id_map ON user_id_map.old_id = user_tokens.userid;",
                key_col = token_columns.key_col,
                hash_col = token_columns.hash_col
            );
            db.execute_unprepared(&insert_sql).await?;
        }

        let has_ownership = sqlite_table_exists(db, "ownership").await?;
        if has_ownership {
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
                SELECT ownership.id, ownership.zoneid, user_id_map.new_id
                FROM ownership
                JOIN user_id_map ON user_id_map.old_id = ownership.userid;",
            )
            .await?;
        }

        if has_user_tokens {
            db.execute_unprepared("DROP TABLE user_tokens;").await?;
            db.execute_unprepared("ALTER TABLE user_tokens_new RENAME TO user_tokens;")
                .await?;
        }
        if has_ownership {
            db.execute_unprepared("DROP TABLE ownership;").await?;
            db.execute_unprepared("ALTER TABLE ownership_new RENAME TO ownership;")
                .await?;
        }

        db.execute_unprepared("DROP TABLE users;").await?;
        db.execute_unprepared("ALTER TABLE users_new RENAME TO users;")
            .await?;
        db.execute_unprepared("DROP TABLE user_id_map;").await?;

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

struct UserTokensColumns {
    key_col: &'static str,
    hash_col: &'static str,
}

async fn user_tokens_columns<C: ConnectionTrait>(db: &C) -> Result<UserTokensColumns, DbErr> {
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA table_info(user_tokens);",
        ))
        .await?;
    let mut key_col: Option<&'static str> = None;
    let mut hash_col: Option<&'static str> = None;
    for row in rows {
        let column_name: String = row.try_get("", "name")?;
        match column_name.as_str() {
            "key" => key_col = Some("key"),
            "tokenkey" => key_col = Some("tokenkey"),
            "hash" => hash_col = Some("hash"),
            "tokenhash" => hash_col = Some("tokenhash"),
            _ => {}
        }
    }

    let key_col = key_col.ok_or_else(|| {
        DbErr::Custom("user_tokens missing key/tokenkey column for migration".to_string())
    })?;
    let hash_col = hash_col.ok_or_else(|| {
        DbErr::Custom("user_tokens missing hash/tokenhash column for migration".to_string())
    })?;

    Ok(UserTokensColumns { key_col, hash_col })
}

async fn users_id_is_integer<C: ConnectionTrait>(db: &C) -> Result<bool, DbErr> {
    if !sqlite_table_exists(db, "users").await? {
        return Ok(false);
    }

    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA table_info(users);",
        ))
        .await?;
    for row in rows {
        let column_name: String = row.try_get("", "name")?;
        if column_name == "id" {
            let column_type: String = row.try_get("", "type")?;
            return Ok(column_type.to_ascii_lowercase().contains("int"));
        }
    }

    Ok(false)
}

async fn users_id_has_text_values<C: ConnectionTrait>(db: &C) -> Result<bool, DbErr> {
    if !sqlite_table_exists(db, "users").await? {
        return Ok(false);
    }

    let row = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) as cnt FROM users WHERE typeof(id) = 'text';",
        ))
        .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let cnt: i64 = row.try_get("", "cnt")?;
    Ok(cnt > 0)
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum UsersNew {
    Table,
    Id,
    Displayname,
    Username,
    Email,
    Disabled,
    Authref,
    Admin,
}

#[derive(DeriveIden)]
enum UserTokensNew {
    Table,
    Id,
    Name,
    Issued,
    Expiry,
    Key,
    Hash,
    Userid,
}

#[derive(DeriveIden)]
enum OwnershipNew {
    Table,
    Id,
    Zoneid,
    Userid,
}

#[derive(DeriveIden)]
enum Zones {
    Table,
    Id,
}

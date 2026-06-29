use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use goatns::config::ConfigFile;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::SessionStore;
use tower_sessions_sqlx_store::SqliteStore;

const SESSION_USER_KEY: &str = "user";

#[derive(Parser)]
struct Opts {
    #[clap(long, env = "GOATNS_CONFIG_FILE")]
    config_file: PathBuf,
    #[clap(long)]
    username: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Opts::parse();

    let config = ConfigFile::try_as_cowcell(Some(opts.config_file))?;

    let dbconn = goatns::db::get_conn(config.read().await)
        .await
        .map_err(|e| format!("Failed to connect to database: {e:?}"))?;

    let existing = goatns::db::entities::users::Entity::find()
        .filter(goatns::db::entities::users::Column::Username.eq(&opts.username))
        .one(&dbconn)
        .await?;

    let user_model = match existing {
        Some(user) => goatns::db::entities::users::Model {
            admin: true,
            ..user
        },
        None => {
            goatns::db::entities::users::ActiveModel {
                id: NotSet,
                displayname: Set(opts.username.clone()),
                username: Set(opts.username.clone()),
                email: Set(format!("{}@test.local", opts.username)),
                disabled: Set(false),
                authref: Set(None),
                admin: Set(true),
            }
            .insert(&dbconn)
            .await?
        }
    };

    let user_json = serde_json::json!({
        "id": user_model.id,
        "displayname": user_model.displayname,
        "username": user_model.username,
        "email": user_model.email,
        "disabled": user_model.disabled,
        "authref": user_model.authref,
        "admin": user_model.admin,
    });

    let pool = dbconn.get_sqlite_connection_pool().clone();
    let session_store = SqliteStore::new(pool).with_table_name("sessions")?;
    session_store.migrate().await?;

    let mut data = HashMap::new();
    data.insert(SESSION_USER_KEY.to_string(), user_json);

    let record = Record {
        id: Id::default(),
        data,
        expiry_date: OffsetDateTime::now_utc() + time::Duration::days(1),
    };

    let session_id = record.id.to_string();
    session_store.save(&record).await?;

    println!("{}", session_id);
    Ok(())
}

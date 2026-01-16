use crate::error::GoatNsError;
use std::time::Duration;

use crate::config::ConfigFile;

use concread::cowcell::asynch::CowCellReadTxn;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::*;

pub(crate) mod entities;
pub mod migrations;
#[cfg(test)]
pub mod test;

async fn get_conn_inner(
    db_url: &str,
    log_sql_statements: bool,
) -> Result<DatabaseConnection, GoatNsError> {
    debug!("Opening Database: {db_url}");
    let mut opt = ConnectOptions::new(db_url);

    opt.connect_timeout(Duration::from_secs(3))
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(1))
        .max_lifetime(Duration::from_secs(30))
        .sqlx_logging(log_sql_statements);
    Database::connect(opt).await.map_err(GoatNsError::from)
}

/// Setup the database connection and pool
pub async fn get_conn(
    config_reader: CowCellReadTxn<ConfigFile>,
) -> Result<DatabaseConnection, GoatNsError> {
    let db_path: &str = &shellexpand::full(&config_reader.db_path)
        .map_err(|err| GoatNsError::StartupError(err.to_string()))?;
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    let db = get_conn_inner(&db_url, config_reader.sql_log_statements).await?;
    use sea_orm_migration::MigratorTrait;
    // Run ALL the migrations
    crate::db::migrations::Migrator::up(&db, None).await?;
    info!("Completed DB Startup!");
    Ok(db)
}

#[cfg(test)]
/// Get a sqlite pool with a memory-only database
pub async fn test_get_sqlite_memory() -> DatabaseConnection {
    crate::init_crypto();

    let db = get_conn_inner("sqlite::memory:", false)
        .await
        .expect("Failed to connect to sqlite memory");
    use crate::db::migrations::Migrator;
    use sea_orm_migration::prelude::*;

    // Apply all pending migrations
    Migrator::up(&db, None)
        .await
        .expect("failed to run migrations");
    db
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenSearchRow {
    pub tokenhash: String,
    pub user: entities::users::Model,
    pub tokenkey: String,
}

/// Run this periodically to clean up expired DB things
pub async fn cron_db_cleanup(pool: DatabaseConnection, period: Duration, max_iter: Option<usize>) {
    let mut interval = time::interval(period);
    let mut iterations = 0;
    loop {
        interval.tick().await;

        if let Err(error) = entities::user_tokens::Entity::cleanup(&pool).await {
            error!("Failed to clean up UserAuthToken objects in DB cron: {error:?}");
        }
        if let Some(max_iter) = max_iter {
            iterations += 1;
            if iterations >= max_iter {
                break;
            }
        }
    }
}

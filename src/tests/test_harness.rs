use sqlx::{Pool, Sqlite};

use crate::datastore::handle_import_file;

pub async fn import_test_zone_file(pool: &Pool<Sqlite>) -> Result<(), String> {
    handle_import_file(
        &pool,
        "./examples/test_config/zones.json".to_string(),
        Some("hello.goat".to_string()),
    )
    .await
    .map_err(|e| format!("Failed to import test zones.json: {e:?}"))
}

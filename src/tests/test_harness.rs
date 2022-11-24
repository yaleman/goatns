use sqlx::{Pool, Sqlite};

use crate::datastore::handle_import_file;
use crate::db::{DBEntity, User};

pub async fn import_test_zone_file(pool: &Pool<Sqlite>) -> Result<(), String> {
    handle_import_file(
        &pool,
        "./examples/test_config/zones.json".to_string(),
        Some("hello.goat".to_string()),
    )
    .await
    .map_err(|e| format!("Failed to import test zones.json: {e:?}"))
}

pub async fn create_test_user(pool: &Pool<Sqlite>) -> Result<Box<User>, sqlx::Error> {
    println!("Creating User");
    Ok(User {
        id: None,
        displayname: "Testuser".to_string(),
        username: "testuser".to_string(),
        email: "billy@dotgoat.net".to_string(),
        disabled: false,
        authref: None,
        admin: false,
    }
    .save(&pool)
    .await?)
}

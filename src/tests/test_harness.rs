use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, DatabaseConnection};

use crate::datastore::handle_import_file;
use crate::db::entities;
use crate::error::GoatNsError;

pub async fn import_test_zone_file(pool: &DatabaseConnection) -> Result<(), GoatNsError> {
    println!("#####################################################################");
    println!("importing test zone ./examples/test_config/zones.json");
    println!("#####################################################################");
    handle_import_file(
        pool,
        "./examples/test_config/zones.json".to_string(),
        Some("hello.goat".to_string()),
    )
    .await
    .map_err(|e| GoatNsError::Generic(format!("Failed to import test zones.json: {e:?}")))?;
    println!("#####################################################################");
    println!("finished importing test zone ./examples/test_config/zones.json");
    println!("#####################################################################");
    Ok(())
}

pub async fn create_test_user(pool: &DatabaseConnection) -> entities::users::Model {
    println!("Creating User");
    let res = entities::users::ActiveModel {
        id: NotSet,
        displayname: Set("Testuser".to_string()),
        username: Set("testuser".to_string()),
        email: Set("billy@dotgoat.net".to_string()),
        disabled: Set(false),
        authref: Set(None),
        admin: Set(false),
    }
    .insert(pool)
    .await;
    res.expect("Failed to create test user")
}

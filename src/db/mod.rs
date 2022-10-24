// https://diesel.rs/guides/getting-started
use crate::config::ConfigFile;
use diesel::prelude::*;

#[cfg(test)]
pub mod tests;

pub mod models;
pub mod schema;
use schema::zones;

fn establish_connection(config: &ConfigFile) -> SqliteConnection {
    SqliteConnection::establish(&config.sqlite_path)
        .unwrap_or_else(|_| panic!("Error connecting to {}", config.sqlite_path))
}

pub fn create_zone(name: &str, config: &ConfigFile) -> usize {
    let mut connection = establish_connection(config);

    let new_zone = models::NewZone { name };

    diesel::insert_into(zones::table)
        .values(&new_zone)
        .execute(&mut connection)
        .expect("Error saving new post")
}

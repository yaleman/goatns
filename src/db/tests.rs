/// Database Connection tests

#[test]
pub fn test_establish_connection() {
    use diesel::prelude::*;

    let dbstring = "sqlite://:memory:";

    // let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let conn = SqliteConnection::establish(dbstring);
    assert!(conn.is_ok());
    // .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

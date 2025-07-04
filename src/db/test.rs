use super::*;

use crate::enums::{RecordClass, RecordType};
use crate::error::GoatNsError;
use crate::zones::{FileZone, FileZoneRecord};

#[tokio::test]
async fn create_user() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await?;

    let mut user = User {
        username: "yaleman".to_string(),
        email: "billy@hello.goat".to_string(),
        disabled: true,
        ..User::default()
    };

    println!("Creating user the first time");
    user.save(&pool).await?;

    user.disabled = false;

    println!("Creating user the second time");
    let res = user.save(&pool).await;
    assert!(res.is_err());

    Ok(())
}

#[cfg(test)]
/// create a zone example.com
pub async fn test_create_example_com_records(
    pool: &SqlitePool,
    zoneid: i64,
    num_records: usize,
) -> Result<(), GoatNsError> {
    use rand::distr::{Alphanumeric, SampleString};

    let mut name: String;
    let mut rdata: String;
    for i in 0..num_records {
        name = Alphanumeric.sample_string(&mut rand::rng(), 16);
        rdata = Alphanumeric.sample_string(&mut rand::rng(), 32);

        FileZoneRecord {
            zoneid: Some(zoneid),
            name,
            rrtype: RecordType::A.to_string(),
            class: RecordClass::Internet,
            rdata,
            id: None,
            ttl: i as u32,
        }
        .save(pool)
        .await?;
    }
    println!("Completed creating records");
    Ok(())
}

#[tokio::test]
async fn test_get_zone_records() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;
    start_db(&pool).await?;
    test_create_example_com_zone(&pool).await?;
    let testzone = test_example_com_zone();

    let mut txn = pool.begin().await?;
    let zone = FileZone::get_by_name(&mut txn, &testzone.name)
        .await?
        .expect("Couldn't get zone");

    test_create_example_com_records(&pool, zone.id.expect("Zone ID not found"), 1000).await?;

    let zone = FileZone::get_by_name(&mut txn, &testzone.name)
        .await?
        .expect("Failed to get zone")
        .with_zone_records(&mut txn)
        .await;

    assert_eq!(zone.records.len(), 1000);
    Ok(())
}

/// Checks that the table create process works and is idempotent
#[tokio::test]
async fn test_db_create_table_zones() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;
    FileZone::create_table(&pool).await?;
    FileZone::create_table(&pool).await?;
    FileZone::create_table(&pool).await
}

/// Checks that the table create process works and is idempotent
#[tokio::test]
async fn test_db_create_table_records() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;
    println!("Creating Records Table");
    FileZoneRecord::create_table(&pool).await?;
    FileZoneRecord::create_table(&pool).await?;
    FileZoneRecord::create_table(&pool).await
}

/// An example zone for testing
pub fn test_example_com_zone() -> FileZone {
    FileZone {
        id: Some(1),
        name: String::from("example.com"),
        rname: String::from("billy.example.com"),
        ..FileZone::default()
    }
}

/// Get a sqlite pool with a memory-only database
pub async fn test_get_sqlite_memory() -> SqlitePool {
    SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to sqlite memory")
}

/// A whole lotta tests
#[tokio::test]
async fn test_db_create_records() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await?;

    println!("Creating Zone");
    let zone = test_example_com_zone()
        .save(&pool)
        .await
        .expect("Failed to save the zone!");

    let mut txn = pool.begin().await?;
    eprintln!(
        "Zone after create: {:?}",
        FileZone::get_by_name(&mut txn, &test_example_com_zone().name).await?
    );

    println!("Creating Record");
    let rrtype: &str = RecordType::TXT.into();
    let rec_to_create = FileZoneRecord {
        name: "foo".to_string(),
        zoneid: zone.id,
        ttl: 123,
        id: None,
        rrtype: rrtype.into(),
        class: RecordClass::Internet,
        rdata: "test txt".to_string(),
    };
    println!("rec to create: {rec_to_create:?}");
    if let Err(error) = rec_to_create.save(&pool).await {
        panic!("{error:?}");
    };

    let res = get_records(
        &pool,
        "foo".to_string(),
        RecordType::TXT,
        RecordClass::Internet,
        false,
    )
    .await?;
    println!("Record: {res:?}");
    Ok(())
}

/// test all the things
#[tokio::test]
async fn test_all_db_things() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    println!("Creating Zones Table");
    FileZone::create_table(&pool).await?;
    println!("Creating Records Table");
    FileZoneRecord::create_table(&pool).await?;
    println!("Successfully created tables!");

    let zone = test_example_com_zone();

    println!("Creating a zone");
    zone.clone().save(&pool).await?;
    println!("Getting a zone!");
    let mut txn = pool.begin().await?;
    let zone_data = FileZone::get_by_name(&mut txn, "example.com")
        .await?
        .expect("Failed to get zone");
    println!("Zone: {:?}", zone_data);

    assert_eq!(*zone_data, zone);
    let zone_data = FileZone::get_by_name(&mut txn, "example.com")
        .await?
        .expect("Failed to get zone");
    println!("{:?}", zone_data);
    assert_eq!(*zone_data, zone);

    println!("Creating Record");
    let rrtype: &str = RecordType::TXT.into();
    let rec_to_create = FileZoneRecord {
        name: "foo".to_string(),
        ttl: 123,
        zoneid: Some(1),
        id: None,
        rrtype: rrtype.into(),
        class: RecordClass::Internet,
        rdata: "test txt".to_string(),
    };
    println!("rec to create: {rec_to_create:?}");
    if let Err(err) = rec_to_create.save(&pool).await {
        panic!("{err:?}");
    };
    if let Err(err) = rec_to_create.save(&pool).await {
        panic!("{err:?}");
    };
    // rec_to_create.save(&pool).await?;
    // rec_to_create.save(&pool).await?;

    println!("Looking for foo.example.com TXT IN");
    let result = get_records(
        &pool,
        String::from("foo.example.com"),
        RecordType::TXT,
        RecordClass::Internet,
        false,
    )
    .await?;
    println!("Result: {result:?}");
    assert!(!result.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_load_zone() -> Result<(), GoatNsError> {
    let mut zone = FileZone {
        name: "example.com".to_string(),
        rname: "billy.example.com".to_string(),
        ..Default::default()
    };

    let pool = test_get_sqlite_memory().await;
    start_db(&pool).await?;

    // first time
    zone.save(&pool).await?;

    let zone_first = FileZone::get_by_name(&mut *pool.begin().await?, &zone.name)
        .await?
        .expect("Couldn't find zone!");

    zone.rname = "foo.example.com".to_string();
    zone.save(&pool).await?;

    let zone_second = FileZone::get_by_name(&mut *pool.begin().await?, &zone.name)
        .await?
        .expect("Couldn't find zone!");

    assert_ne!(zone_first, zone_second);

    // compare the record lists
    println!("comparing the list of records in each zone");
    for record in zone_first.records.iter() {
        assert!(zone_second.records.contains(record));
    }
    for record in zone_second.records.iter() {
        assert!(zone_first.records.contains(record));
    }

    Ok(())
}

#[cfg(test)]
/// create a zone example.com
async fn test_create_example_com_zone(pool: &SqlitePool) -> Result<(), GoatNsError> {
    test_example_com_zone().save(pool).await?;
    Ok(())
}

#[tokio::test]
async fn test_export_zone() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;
    eprintln!("Setting up DB");
    start_db(&pool).await?;
    eprintln!("Setting up example zone");
    test_create_example_com_zone(&pool).await?;
    let testzone = test_example_com_zone();

    eprintln!("Getting example zone");
    let zone = FileZone::get_by_name(&mut *pool.begin().await?, &testzone.name)
        .await?
        .expect("Failed to get zone");

    let records_to_create = 100usize;
    eprintln!("Creating records");
    if let Err(err) = test_create_example_com_records(
        &pool,
        zone.id.expect("Failed to get zone id"),
        records_to_create,
    )
    .await
    {
        panic!("failed to create test records: {err:?}");
    }

    eprintln!("Exporting zone {}", zone.id.expect("Failed to get zone id"));
    let exported_zone = FileZone::get(&pool, zone.id.expect("Failed to get zone id")).await?;
    eprintln!("Done exporting zone");

    println!("found {} records", exported_zone.records.len());
    assert_eq!(exported_zone.records.len(), records_to_create);

    let json_result =
        serde_json::to_string_pretty(&exported_zone).expect("Failed to convert to json");

    println!("{json_result}");

    let export_json_result =
        match export_zone_json(&pool, zone.id.expect("Failed to get zone id")).await {
            Ok(val) => val,
            Err(err) => panic!("error exporting json: {err}"),
        };

    println!("Checking that the result matches expectation");
    assert_eq!(json_result, export_json_result);

    Ok(())
}

#[tokio::test]
async fn load_then_export() -> Result<(), GoatNsError> {
    use tokio::io::AsyncReadExt;
    // set up the DB
    let pool = test_get_sqlite_memory().await;
    eprintln!("Setting up DB");
    start_db(&pool).await?;

    let example_zone_file = std::path::Path::new(&"./examples/test_config/single-zone.json");

    eprintln!("load_zone_from_file from {:?}", example_zone_file);
    let example_zone = crate::zones::load_zone_from_file(example_zone_file)
        .inspect_err(|err| println!("Failed to load zone file! {:?}", err))?;

    eprint!("importing zone into db...");
    example_zone.save(&pool).await?;
    eprintln!("done!");

    let mut file = match tokio::fs::File::open(example_zone_file).await {
        Ok(value) => value,
        Err(error) => {
            panic!("Failed to open zone file: {:?}", error);
        }
    };
    let mut buf: String = String::new();
    file.read_to_string(&mut buf)
        .await
        .expect("Failed to read test file");

    eprintln!("File contents: {:?}", buf);

    let json: FileZone = json5::from_str(&buf)
        .map_err(|e| panic!("{e:?}"))
        .expect("Failed to parse json");
    eprintln!("loaded zone from file again: {json:?}");
    let _json: String = serde_json::to_string(&json).expect("Failed to convert to json");

    eprintln!("Exporting zone");
    let zone_got = FileZone::get_by_name(&mut *pool.begin().await?, &example_zone.name).await?;
    eprintln!("zone_got {zone_got:?}");

    if let Err(err) = export_zone_json(&pool, 1).await {
        panic!("Failed to export zone! {err}");
    }

    Ok(())
}

#[tokio::test]
async fn test_zone_ownership_get_by_name() -> Result<(), GoatNsError> {
    // Set up the database
    let pool = test_get_sqlite_memory().await;
    start_db(&pool).await?;
    
    // Create a test user
    let user = User {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        disabled: false,
        ..User::default()
    };
    let saved_user = user.save(&pool).await?;
    let user_id = saved_user.id.expect("User should have an ID");
    
    // Create a test zone
    let zone = FileZone {
        name: "example.com".to_string(),
        rname: "admin.example.com".to_string(),
        serial: 1,
        refresh: 3600,
        retry: 1800,
        expire: 604800,
        minimum: 86400,
        ..FileZone::default()
    };
    let saved_zone = zone.save(&pool).await?;
    let zone_id = saved_zone.id.expect("Zone should have an ID");
    
    // Create zone ownership
    let ownership = ZoneOwnership {
        id: None,
        userid: user_id,
        zoneid: zone_id,
    };
    ownership.save(&pool).await?;
    
    // Test get_by_name
    let mut conn = pool.begin().await?;
    let found_ownership = ZoneOwnership::get_by_name(&mut conn, "example.com").await?;
    
    assert!(found_ownership.is_some());
    let ownership_record = found_ownership.unwrap();
    assert_eq!(ownership_record.userid, user_id);
    assert_eq!(ownership_record.zoneid, zone_id);
    
    // Test with non-existent zone
    let not_found = ZoneOwnership::get_by_name(&mut conn, "nonexistent.com").await?;
    assert!(not_found.is_none());
    
    conn.commit().await?;
    Ok(())
}

#[tokio::test]
async fn test_zone_ownership_get_all_by_name() -> Result<(), GoatNsError> {
    // Set up the database
    let pool = test_get_sqlite_memory().await;
    start_db(&pool).await?;
    
    // Create test users
    let user1 = User {
        username: "testuser1".to_string(),
        email: "test1@example.com".to_string(),
        disabled: false,
        ..User::default()
    };
    let saved_user1 = user1.save(&pool).await?;
    let user1_id = saved_user1.id.expect("User should have an ID");
    
    let user2 = User {
        username: "testuser2".to_string(),
        email: "test2@example.com".to_string(),
        disabled: false,
        ..User::default()
    };
    let saved_user2 = user2.save(&pool).await?;
    let user2_id = saved_user2.id.expect("User should have an ID");
    
    // Create a test zone
    let zone = FileZone {
        name: "shared.com".to_string(),
        rname: "admin.shared.com".to_string(),
        serial: 1,
        refresh: 3600,
        retry: 1800,
        expire: 604800,
        minimum: 86400,
        ..FileZone::default()
    };
    let saved_zone = zone.save(&pool).await?;
    let zone_id = saved_zone.id.expect("Zone should have an ID");
    
    // Create multiple zone ownerships for the same zone
    let ownership1 = ZoneOwnership {
        id: None,
        userid: user1_id,
        zoneid: zone_id,
    };
    ownership1.save(&pool).await?;
    
    let ownership2 = ZoneOwnership {
        id: None,
        userid: user2_id,
        zoneid: zone_id,
    };
    ownership2.save(&pool).await?;
    
    // Test get_all_by_name
    let mut conn = pool.begin().await?;
    let all_ownerships = ZoneOwnership::get_all_by_name(&mut conn, "shared.com").await?;
    
    assert_eq!(all_ownerships.len(), 2);
    
    // Verify both users are in the ownership list
    let user_ids: Vec<i64> = all_ownerships.iter().map(|o| o.userid).collect();
    assert!(user_ids.contains(&user1_id));
    assert!(user_ids.contains(&user2_id));
    
    // All should have the same zone ID
    for ownership in &all_ownerships {
        assert_eq!(ownership.zoneid, zone_id);
    }
    
    // Test with non-existent zone
    let empty_result = ZoneOwnership::get_all_by_name(&mut conn, "nonexistent.com").await?;
    assert!(empty_result.is_empty());
    
    conn.commit().await?;
    Ok(())
}

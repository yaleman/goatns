use super::*;

use crate::enums::{RecordClass, RecordType};
use crate::zones::{FileZone, FileZoneRecord};

#[tokio::test]
async fn test_create_user() -> Result<(), sqlx::Error> {
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
async fn test_create_example_com_records(
    pool: &SqlitePool,
    zoneid: u64,
    num_records: usize,
) -> Result<(), sqlx::Error> {
    use rand::distributions::{Alphanumeric, DistString};

    let mut name: String;
    let mut rdata: String;
    for i in 0..num_records {
        name = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        rdata = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

        FileZoneRecord {
            zoneid,
            name,
            rrtype: RecordType::A.to_string(),
            class: RecordClass::Internet,
            rdata,
            id: i as u64,
            ttl: i as u32,
        }
        .save(&pool)
        .await?;
    }
    println!("Completed creating records");
    Ok(())
}

#[tokio::test]
async fn test_get_zone_records() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;
    start_db(&pool).await?;
    test_create_example_com_zone(&pool).await?;
    let testzone = test_example_com_zone();

    let zone = match get_zone(&pool, testzone.name).await? {
        Some(value) => value,
        None => return Err(sqlx::Error::RowNotFound),
    };

    test_create_example_com_records(&pool, zone.id, 1000).await?;

    let records = zone.get_zone_records(&mut pool.begin().await?).await?;
    for record in &records {
        eprintln!("{}", record);
    }

    assert_eq!(records.len(), 1000);
    Ok(())
}

/// Ensures that when we ask for something that isn't there, it returns None
#[tokio::test]

async fn test_get_zone_empty() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;
    println!("Creating Zones Table");
    create_zones_table(&pool).await?;
    let zone_data = get_zone(&pool, "example.org".to_string()).await?;
    println!("{:?}", zone_data);
    assert_eq!(zone_data, None);
    Ok(())
}

/// Checks that the table create process works and is idempotent
#[tokio::test]
async fn test_db_create_table_zones() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;
    create_zones_table(&pool).await?;
    create_zones_table(&pool).await?;
    Ok(create_zones_table(&pool).await?)
}

/// Checks that the table create process works and is idempotent
#[tokio::test]
async fn test_db_create_table_records() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;
    println!("Creating Records Table");
    create_records_table(&pool).await?;
    create_records_table(&pool).await?;
    Ok(create_records_table(&pool).await?)
}

/// An example zone for testing
#[cfg(test)]
pub fn test_example_com_zone() -> FileZone {
    FileZone {
        id: 1,
        name: String::from("example.com"),
        rname: String::from("billy.example.com"),
        ..FileZone::default()
    }
}

/// Get a sqlite pool with a memory-only database
#[cfg(test)]
pub async fn test_get_sqlite_memory() -> SqlitePool {
    SqlitePool::connect("sqlite::memory:").await.unwrap()
}

/// A whole lotta tests
#[tokio::test]
async fn test_db_create_records() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await?;

    println!("Creating Zone");
    let zoneid = match test_example_com_zone().save(&pool).await {
        Ok(value) => value,
        Err(err) => panic!("{err:?}"),
    };

    eprintln!(
        "Zone after create: {:?}",
        get_zone(&pool, test_example_com_zone().name).await?
    );

    println!("Creating Record");
    let rrtype: &str = RecordType::TXT.into();
    let rec_to_create = FileZoneRecord {
        name: "foo".to_string(),
        zoneid,
        ttl: 123,
        id: 1,
        rrtype: rrtype.into(),
        class: RecordClass::Internet.into(),
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
    )
    .await?;
    println!("Record: {res:?}");
    Ok(())
}

/// test all the things
#[tokio::test]
async fn test_all_db_things() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;

    println!("Creating Zones Table");
    create_zones_table(&pool).await?;
    println!("Creating Records Table");
    create_records_table(&pool).await?;
    println!("Successfully created tables!");

    let zone = test_example_com_zone();

    println!("Creating a zone");
    zone.clone().save(&pool).await?;
    println!("Getting a zone!");

    let zone_data = get_zone(&pool, "example.com".to_string()).await?;
    println!("Zone: {:?}", zone_data);
    assert_eq!(zone_data, Some(zone));
    let zone_data = get_zone(&pool, "example.org".to_string()).await?;
    println!("{:?}", zone_data);
    assert_eq!(zone_data, None);

    println!("Creating Record");
    let rrtype: &str = RecordType::TXT.into();
    let rec_to_create = FileZoneRecord {
        name: "foo".to_string(),
        ttl: 123,
        zoneid: 1,
        id: 1,
        rrtype: rrtype.into(),
        class: RecordClass::Internet.into(),
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
    )
    .await?;
    println!("Result: {result:?}");
    assert!(!result.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_load_zone() -> Result<(), sqlx::Error> {
    let mut zone = FileZone {
        id: 0,
        name: "example.com".to_string(),
        rname: "billy.example.com".to_string(),
        ..Default::default()
    };

    let pool = test_get_sqlite_memory().await;
    start_db(&pool).await?;

    // first time
    zone.save(&pool).await?;

    let zone_first = get_zone(&pool, zone.to_owned().name).await?.unwrap();

    zone.rname = "foo.example.com".to_string();
    zone.save(&pool).await?;

    let zone_second = get_zone(&pool, zone.clone().name).await?.unwrap();

    assert_ne!(zone_first, zone_second);

    // compare the record lists
    println!("comparing the list of records in each zone");
    for record in zone_first.records.iter() {
        assert!(zone_second.records.contains(&record));
    }
    for record in zone_second.records.iter() {
        assert!(zone_first.records.contains(&record));
    }

    Ok(())
}

#[cfg(test)]
/// create a zone example.com
async fn test_create_example_com_zone(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    test_example_com_zone().save(&pool).await?;
    Ok(())
}

#[tokio::test]
async fn test_export_zone() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;
    eprintln!("Setting up DB");
    start_db(&pool).await?;
    eprintln!("Setting up example zone");
    test_create_example_com_zone(&pool).await?;
    let testzone = test_example_com_zone();

    eprintln!("Getting example zone");
    let zone = match get_zone(&pool, testzone.clone().name).await? {
        Some(value) => value,
        None => {
            println!("couldn't find zone {}", testzone.name);
            return Err(sqlx::Error::RowNotFound);
        }
    };

    let records_to_create = 100usize;
    eprintln!("Creating records");
    if let Err(err) = test_create_example_com_records(&pool, zone.id, records_to_create).await {
        panic!("failed to create test records: {err:?}");
    }

    eprintln!("Exporting zone {}", zone.id);
    let exported_zone = export_zone(pool.acquire().await?, zone.id.try_into().unwrap()).await?;
    eprintln!("Done exporting zone");

    println!("found {} records", exported_zone.records.len());
    assert_eq!(exported_zone.records.len(), records_to_create);

    let json_result = serde_json::to_string_pretty(&exported_zone).unwrap();

    println!("{json_result}");

    let export_json_result =
        match export_zone_json(pool.acquire().await?, zone.id.try_into().unwrap()).await {
            Ok(val) => val,
            Err(err) => panic!("error exporting json: {err}"),
        };

    println!("Checking that the result matches expectation");
    assert_eq!(json_result, export_json_result);

    Ok(())
}

#[tokio::test]
async fn load_then_export() -> Result<(), sqlx::Error> {
    use tokio::io::AsyncReadExt;
    // set up the DB
    let pool = test_get_sqlite_memory().await;
    eprintln!("Setting up DB");
    start_db(&pool).await?;

    // let example_zone_file = std::fs::Path:: ::from("./examples/test_config/single-zone.json");
    // let example_zone_file = example_zone_file.as_path();
    // if !example_zone_file.exists() {
    // panic!("couldn't find example zone file {:?}", example_zone_file);
    // }
    let example_zone_file = std::path::Path::new(&"./examples/test_config/single-zone.json");

    eprintln!("load_zone_from_file from {:?}", example_zone_file);
    let example_zone = match crate::zones::load_zone_from_file(example_zone_file) {
        Ok(value) => value,
        Err(error) => panic!("Failed to load zone file! {:?}", error),
    };

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
    file.read_to_string(&mut buf).await.unwrap();

    eprintln!("File contents: {:?}", buf);

    let json: FileZone = json5::from_str(&buf).map_err(|e| panic!("{e:?}")).unwrap();
    eprintln!("loaded zone from file again: {json:?}");
    let _json: String = serde_json::to_string(&json).unwrap();

    eprintln!("Exporting zone");
    let zone_got = get_zone(&pool, example_zone.clone().name).await?;
    eprintln!("zone_got {zone_got:?}");

    if let Err(err) = export_zone_json(pool.acquire().await?, 1).await {
        panic!("Failed to export zone! {err}");
    }

    Ok(())
}

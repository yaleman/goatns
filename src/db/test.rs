use crate::datastore::import_zonefile;
use crate::tests::prelude::*;
use crate::zones::ZoneFile;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, ModelTrait, QueryFilter,
};

use crate::enums::{RecordClass, RecordType};
use crate::error::GoatNsError;

#[tokio::test]
async fn create_user() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    let user = entities::users::ActiveModel {
        id: NotSet,
        username: Set("yaleman".to_string()),
        email: Set("billy@hello.goat".to_string()),
        disabled: Set(true),
        admin: Set(false),
        displayname: Set("Billy".to_string()),
        authref: Set(Some("authref_value".to_string())),
    };

    println!("Creating user the first time");
    let model = user.save(&pool).await?;

    let mut user = model.into_active_model();

    user.disabled = Set(false);

    println!("Updating user to disable second time");
    let res = user.save(&pool).await;
    assert!(res.is_ok());

    Ok(())
}

#[cfg(test)]
/// create a zone example.com
pub async fn test_create_example_com_records(
    pool: &DatabaseConnection,
    zoneid: Uuid,
    num_records: usize,
) -> Result<(), GoatNsError> {
    use rand::distr::{Alphanumeric, SampleString};

    let mut name: String;
    let mut rdata: String;
    for i in 0..num_records {
        name = Alphanumeric.sample_string(&mut rand::rng(), 16);
        rdata = Alphanumeric.sample_string(&mut rand::rng(), 32);

        entities::records::ActiveModel {
            zoneid: Set(zoneid),
            name: Set(name),
            rrtype: Set(RecordType::A.into()),
            rclass: Set(RecordClass::Internet.into()),
            rdata: Set(rdata),
            id: NotSet,
            ttl: Set(Some(i as u32)),
        }
        .insert(pool)
        .await?;
    }
    println!("Completed creating records");
    Ok(())
}

#[tokio::test]
async fn test_get_zone_records() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;
    let zone = test_create_example_com_zone(&pool)
        .await
        .expect("Failed to create example.com zone");

    test_create_example_com_records(&pool, zone.id, 1000).await?;

    let records = zone
        .find_related(entities::records::Entity)
        .all(&pool)
        .await
        .expect("Failed to get records");

    assert_eq!(records.len(), 1000);
    Ok(())
}

/// An example zone for testing
pub fn test_example_com_zone() -> entities::zones::ActiveModel {
    entities::zones::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(String::from("example.com")),
        rname: Set(String::from("billy.example.com")),
        serial: Set(0),
        refresh: Set(0),
        retry: Set(0),
        expire: Set(0),
        minimum: Set(0),
    }
}
/// A whole lotta tests
#[tokio::test]
async fn test_db_create_records() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    println!("Creating Zone");
    let zone = test_example_com_zone()
        .insert(&pool)
        .await
        .expect("Failed to save the zone!");

    println!("Creating Record");
    let rec_to_create = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone.id),
        name: Set("foo".to_string()),
        ttl: Set(Some(123)),
        rclass: Set(RecordClass::Internet.into()),
        rrtype: Set(RecordType::TXT.into()),
        rdata: Set("test txt".to_string()),
    };
    println!("rec to create: {rec_to_create:?}");
    if let Err(error) = rec_to_create.save(&pool).await {
        panic!("{error:?}");
    };

    let res = entities::records_merged::Entity::get_records(
        &pool,
        "foo",
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

    let zone = test_example_com_zone();

    println!("Creating a zone");
    let zone = zone.clone().insert(&pool).await?;
    println!("Getting a zone!");

    println!("Zone: {zone:?}");

    let zone_data = entities::zones::Entity::find()
        .filter(entities::zones::Column::Name.eq("example.com".to_string()))
        .one(&pool)
        .await?
        .expect("Failed to get zone");
    println!("{zone_data:?}");
    assert!(zone.eq(&zone_data));

    println!("Creating Record");
    let rec_to_create = entities::records::ActiveModel {
        id: NotSet,
        name: Set("foo".to_string()),
        ttl: Set(Some(123)),
        zoneid: Set(zone.id),
        rrtype: Set(RecordType::TXT.into()),
        rclass: Set(RecordClass::Internet.into()),
        rdata: Set("test txt".to_string()),
    };
    println!("rec to create: {rec_to_create:?}");
    let saved_record = rec_to_create.save(&pool).await?;
    println!("Saved record: {saved_record:?}");

    // Saving the same record object again should work (it has an ID now so it's an update)
    if let Err(err) = saved_record.save(&pool).await {
        panic!("{err:?}");
    };
    // rec_to_create.save(&pool).await?;
    // rec_to_create.save(&pool).await?;

    println!("Looking for foo.example.com TXT IN");
    let result = entities::records_merged::Entity::get_records(
        &pool,
        "foo.example.com",
        RecordType::TXT,
        RecordClass::Internet,
        false,
    )
    .await?;
    println!("Result: {result:?}");
    assert!(!result.is_empty());
    Ok(())
}

#[cfg(test)]
/// create a zone example.com
async fn test_create_example_com_zone(
    pool: &DatabaseConnection,
) -> Result<entities::zones::Model, GoatNsError> {
    test_example_com_zone()
        .insert(pool)
        .await
        .map_err(GoatNsError::from)
}

#[tokio::test]
async fn test_export_zone() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;
    eprintln!("Setting up example zone");
    let zone = test_create_example_com_zone(&pool).await?;

    let records_to_create = 100usize;
    eprintln!("Creating records");
    if let Err(err) = test_create_example_com_records(&pool, zone.id, records_to_create).await {
        panic!("failed to create test records: {err:?}");
    }

    // eprintln!("Exporting zone {}", zone.id);
    // let exported_zone = FileZone::get(&pool, zone.id.expect("Failed to get zone id")).await?;
    // eprintln!("Done exporting zone");

    // println!("found {} records", exported_zone.records.len());
    // assert_eq!(exported_zone.records.len(), records_to_create);

    let json_result = serde_json::to_string_pretty(&zone).expect("Failed to convert to json");

    println!("{json_result}");

    // let export_json_result =
    //     match export_zone_json(&pool, zone.id.expect("Failed to get zone id")).await {
    //         Ok(val) => val,
    //         Err(err) => panic!("error exporting json: {err}"),
    //     };

    // println!("Checking that the result matches expectation");
    // assert_eq!(json_result, export_json_result);

    Ok(())
}

#[tokio::test]
async fn load_then_export() -> Result<(), GoatNsError> {
    use tokio::io::AsyncReadExt;
    // set up the DB
    let pool = test_get_sqlite_memory().await;

    let example_zone_file = std::path::Path::new(&"./examples/test_config/single-zone.json");

    eprintln!("load_zone_from_file from {example_zone_file:?}");
    let example_zone = crate::zones::load_zone_from_file(example_zone_file)
        .inspect_err(|err| println!("Failed to load zone file! {err:?}"))?;
    let zone_name = example_zone.zone.name.clone();
    let am: entities::zones::ActiveModel = example_zone.zone.clone().into();
    am.insert(&pool).await?;
    eprint!("importing zone into db...");
    import_zonefile(&pool, example_zone).await?;
    // example_zone.insert(&pool).await?;
    eprintln!("done!");

    let mut file = match tokio::fs::File::open(example_zone_file).await {
        Ok(value) => value,
        Err(error) => {
            panic!("Failed to open zone file: {error:?}");
        }
    };
    let mut buf: String = String::new();
    file.read_to_string(&mut buf)
        .await
        .expect("Failed to read test file");

    eprintln!("File contents: {buf:?}");

    let json: ZoneFile = json5::from_str(&buf)
        .map_err(|e| panic!("{e:?}"))
        .expect("Failed to parse json");
    eprintln!("loaded zone from file again: {json:?}");
    let _json: String = serde_json::to_string(&json).expect("Failed to convert to json");

    eprintln!("Exporting zone");
    let zone_got = entities::zones::Entity::find()
        .filter(entities::zones::Column::Name.eq(zone_name))
        .one(&pool)
        .await?
        .expect("Failed to get zone from DB");
    eprintln!("zone_got {zone_got:?}");

    Ok(())
}

#[tokio::test]
async fn test_duplicate_record_constraint() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    // Create a test zone
    let test_zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("test.example.com".to_string()),
        rname: Set("admin.example.com".to_string()),
        serial: Set(1),
        refresh: Set(3600),
        retry: Set(1800),
        expire: Set(604800),
        minimum: Set(86400),
    };

    let zone = test_zone.insert(&pool).await?;
    let zone_id = zone.id;

    // Create the first record
    let record1 = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone_id),
        name: Set("www".to_string()),
        rrtype: Set(RecordType::A.into()),
        rclass: Set(RecordClass::Internet.into()),
        rdata: Set("192.168.1.1".to_string()),
        ttl: Set(Some(300)),
    };

    let record1 = record1.insert(&pool).await?;
    println!("Created first record: {record1:?}");

    // Try to create a record with same name, type, class but different rdata (should succeed in DNS)
    let record2 = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone_id),
        name: Set("www".to_string()),
        rrtype: Set(RecordType::A.into()),
        rclass: Set(RecordClass::Internet.into()),
        rdata: Set("192.168.1.2".to_string()),
        ttl: Set(Some(300)),
    };

    let record2 = record2.insert(&pool).await?;
    println!("Created second record: {record2:?}");

    assert_eq!(
        record2.rdata, "192.168.1.2",
        "Second record should have different rdata"
    );

    // Now try to create a truly duplicate record (same name, type, class, AND rdata)
    let record3 = record1.into_active_model();

    record3
        .insert(&pool)
        .await
        .expect_err("Creating duplicate record should fail");

    Ok(())
}

#[tokio::test]
async fn test_record_requires_name() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    // Create a test zone first
    let test_zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("test.example.com".to_string()),
        rname: Set("admin.example.com".to_string()),
        serial: Set(1),
        refresh: Set(3600),
        retry: Set(1800),
        expire: Set(604800),
        minimum: Set(86400),
    };

    let zone = test_zone.insert(&pool).await?;

    // Try to create a record with empty name (now allowed for apex records)
    let record = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone.id),
        name: Set("".to_string()), // Empty name is now allowed for apex records
        rrtype: Set(RecordType::A.into()),
        rclass: Set(RecordClass::Internet.into()),
        rdata: Set("192.168.1.1".to_string()),
        ttl: Set(Some(300)),
    };

    let result = record.insert(&pool).await;

    // This should now succeed (empty names are allowed for apex records)
    match result {
        Ok(saved_record) => {
            eprintln!("Successfully created record with empty name: {saved_record:?}");
            assert_eq!(saved_record.name, "");
        }
        Err(err) => {
            panic!("Expected record creation with empty name to succeed, got: {err:?}");
        }
    }

    Ok(())
}

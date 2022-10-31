use crate::config::ConfigFile;
use crate::enums::{RecordClass, RecordType};

use crate::resourcerecord::InternalResourceRecord;
use crate::zones::FileZone;
use crate::zones::FileZoneRecord;
use sqlx::pool::PoolConnection;
use sqlx::sqlite::SqliteArguments;
use sqlx::{Arguments, Connection, Pool, Row, Sqlite, SqliteConnection, SqlitePool, Transaction};

const SQL_VIEW_RECORDS: &str = "records_merged";

pub async fn get_conn(config: &ConfigFile) -> Result<Pool<Sqlite>, String> {
    let db_path: &str = &shellexpand::full(&config.sqlite_path).unwrap();
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    log::debug!("Opening Database: {db_url}");

    match SqlitePool::connect(&db_url).await {
        Ok(value) => Ok(value),
        Err(err) => Err(format!("Error opening SQLite DB ({db_url:?}): {err:?}")),
    }
}

pub async fn start_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    create_zones_table(pool).await?;

    create_records_table(pool).await?;

    log::info!("Completed DB Startup!");
    Ok(())
}

#[allow(dead_code)]
pub async fn create_zones_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await.unwrap();

    log::info!("Ensuring DB Zones table exists");
    let _res = sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS
        zones (
            id   INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            rname TEXT NOT NULL,
            serial INTEGER NOT NULL,
            refresh INTEGER NOT NULL,
            retry INTEGER NOT NULL,
            expire INTEGER NOT NULL,
            minimum INTEGER NOT NULL
        )"#,
    )
    .execute(&mut tx)
    .await;

    // .execute(tx).await;
    log::info!("Ensuring DB Records index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_zones
        ON zones (
            id,name
        )",
    )
    .execute(&mut tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn create_records_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    log::info!("Ensuring DB Records table exists");

    let mut tx = pool.begin().await.unwrap();

    let _res = sqlx::query(
        "CREATE TABLE IF NOT EXISTS
        records (
            id      INTEGER PRIMARY KEY,
            zoneid  INTEGER NOT NULL,
            name    TEXT, /* this can be null for apex records */
            ttl     INTEGER,
            rtype   INTEGER NOT NULL,
            rclass  INTEGER NOT NULL,
            rdata   TEXT NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id)
        )",
    )
    .execute(&mut tx)
    .await;
    log::info!("Ensuring DB Records index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_records
        ON records (
            id,zoneid,name,rtype,rclass
        )",
    )
    .execute(&mut tx)
    .await?;
    log::info!("Ensuring DB Records view exists");
    // this view lets us query based on the full name
    sqlx::query(
        &format!("CREATE VIEW IF NOT EXISTS {} ( record_id, zone_id, rtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zone_id, records.rtype, records.rclass ,records.rdata,
        CASE
            WHEN records.name is NULL THEN zones.name
            ELSE records.name || '.' || zones.name
        END AS name,
        CASE WHEN records.ttl is NULL then zones.minimum
            WHEN records.ttl > zones.minimum THEN records.ttl
            ELSE records.ttl
        END AS ttl
        from records, zones where records.zoneid = zones.id", SQL_VIEW_RECORDS)
    ).execute(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

#[allow(dead_code)]
/// define a zone
pub async fn create_zone(pool: &SqlitePool, zone: FileZone) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await.unwrap();
    let res = create_zone_with_conn(&mut *tx, zone).await?;
    tx.commit().await?;
    Ok(res)
}

pub async fn create_zone_with_conn(
    conn: &mut SqliteConnection,
    zone: FileZone,
) -> Result<u64, sqlx::Error> {
    let mut args = SqliteArguments::default();
    let serial = zone.serial.to_string();
    let refresh = zone.refresh.to_string();
    let retry = zone.retry.to_string();
    let expire = zone.expire.to_string();
    let minimum = zone.minimum.to_string();
    for arg in [
        &zone.name,
        &zone.rname,
        &serial,
        &refresh,
        &retry,
        &expire,
        &minimum,
    ] {
        args.add(arg);
    }

    let result = sqlx::query_with(
        "INSERT INTO zones (name, rname, serial, refresh, retry, expire, minimum)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        args,
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected())
}

/// create a resource record within a zone
pub async fn create_record(pool: &SqlitePool, record: FileZoneRecord) -> Result<u64, sqlx::Error> {
    let mut txn = pool.begin().await?;
    let res = create_record_with_conn(&mut txn, record).await?;
    txn.commit().await?;
    Ok(res)
}

#[allow(dead_code)]
/// create a resource record within a zone
pub async fn create_record_with_conn(
    txn: &mut Transaction<'_, Sqlite>,
    record: FileZoneRecord,
) -> Result<u64, sqlx::Error> {
    let rclass: u16 = record.class as u16;
    let rrtype = RecordType::from(record.rrtype);
    let rrtype = rrtype as u16;

    let mut args = SqliteArguments::default();
    let input_args: Vec<String> = vec![
        record.zoneid.to_string(),
        record.name,
        record.ttl.to_string(),
        rrtype.to_string(),
        rclass.to_string(),
        record.rdata,
    ];
    for arg in input_args {
        args.add(arg);
    }
    let result = sqlx::query_with(
        "INSERT INTO records (zoneid, name, ttl, rtype, rclass, rdata)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        args,
    )
    .execute(&mut *txn)
    .await?;
    Ok(result.rows_affected())
}

pub async fn get_zone_with_conn(
    txn: &mut Transaction<'_, Sqlite>,
    name: String,
) -> Result<Option<FileZone>, sqlx::Error> {
    let mut args = SqliteArguments::default();

    args.add(name);

    let result = sqlx::query_with(
        "SELECT
        id, name, rname, serial, refresh, retry, expire, minimum
        FROM zones
        WHERE name = ?",
        args,
    )
    .fetch_optional(&mut *txn)
    .await?;
    match result {
        None => Ok(None),
        Some(row) => {
            let id: i64 = row.get(0);

            Ok(Some(FileZone {
                id: id as u64,
                name: row.get(1),
                rname: row.get(2),
                serial: row.get(3),
                refresh: row.get(4),
                retry: row.get(5),
                expire: row.get(6),
                minimum: row.get(7),
                records: vec![],
            }))
        }
    }
}

#[allow(dead_code)]
pub async fn get_zone(pool: &SqlitePool, name: String) -> Result<Option<FileZone>, sqlx::Error> {
    let mut txn = pool.begin().await?;

    get_zone_with_conn(&mut txn, name).await
}

#[allow(dead_code)]
pub async fn update_zone(pool: &SqlitePool, zone: FileZone) -> Result<u64, sqlx::Error> {
    let mut txn = pool.begin().await?;
    let res = update_zone_with_conn(&mut txn, zone).await?;
    txn.commit().await?;
    Ok(res)
}

#[allow(dead_code)]
pub async fn update_zone_with_conn(
    txn: &mut Transaction<'_, Sqlite>,
    zone: FileZone,
) -> Result<u64, sqlx::Error> {
    // let mut tx = pool.begin().await?;

    let mut args = SqliteArguments::default();
    args.add(zone.rname);
    args.add(zone.serial);
    args.add(zone.refresh);
    args.add(zone.retry);
    args.add(zone.expire);
    args.add(zone.minimum);
    args.add(zone.id as f64);

    let qry = sqlx::query_with(
        "UPDATE zones
        set rname = ?, serial = ?, refresh = ?, retry = ?, expire = ?, minimum =?
        WHERE id = ?",
        args,
    )
    .execute(&mut *txn)
    .await?;
    println!("Rows updated: {}", qry.rows_affected());
    Ok(qry.rows_affected())
}

pub async fn get_records(
    conn: &SqlitePool,
    name: String,
    rtype: RecordType,
    rclass: RecordClass,
) -> Result<Vec<InternalResourceRecord>, sqlx::Error> {
    // let mut tx = pool.begin().await?;
    let mut tx = conn.begin().await?;
    let search_rtype = (rtype as u16).to_string();
    let search_rclass = (rclass as u16).to_string();
    let mut args: SqliteArguments = SqliteArguments::default();
    eprintln!("search_rtype {search_rtype} search_rclass {search_rclass}");
    args.add(&name);
    args.add(search_rtype);
    args.add(search_rclass);
    let res = sqlx::query_with(
        &format!(
            "SELECT
        record_id, zone_id, name, rclass, rtype, rdata, ttl
        FROM {}
        WHERE name = ? AND rtype = ? AND rclass = ?",
            SQL_VIEW_RECORDS
        ),
        args,
    )
    .fetch_all(&mut *tx)
    .await?;

    if res.is_empty() {
        log::error!("No results returned for {name}");
    }

    let mut results: Vec<InternalResourceRecord> = vec![];
    for row in res {
        // let record_id: i64 = row.get(0);
        let zoneid: i64 = row.get(1);
        let zoneid: u64 = zoneid.try_into().unwrap_or(0);
        let record_name: String = row.get(2);
        let record_class: u16 = row.get(3);
        let record_type: u16 = row.get(4);
        let rrtype: &str = RecordType::from(&record_type).into();
        let rdata: String = row.get(5);
        let ttl: u32 = row.get(6);
        let irr = InternalResourceRecord::try_from(FileZoneRecord {
            name: record_name,
            ttl,
            zoneid,
            rrtype: rrtype.to_string(),
            class: RecordClass::from(&record_class),
            rdata,
        });
        if irr.is_ok() {
            results.push(irr.unwrap());
        }
    }

    log::trace!("results: {results:?}");
    Ok(results)
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

    println!("Creating Zones Table");
    create_zones_table(&pool).await?;
    println!("Creating Records Table");
    create_records_table(&pool).await?;

    println!("Creating Zone");
    let zoneid = match create_zone(&pool, test_example_com_zone()).await {
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
        ttl: 123,
        zoneid,
        rrtype: rrtype.into(),
        class: RecordClass::Internet.into(),
        rdata: "test txt".to_string(),
    };
    println!("rec to create: {rec_to_create:?}");
    if let Err(error) = create_record(&pool, rec_to_create).await {
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
    create_zone(&pool, zone.clone()).await?;
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
        rrtype: rrtype.into(),
        class: RecordClass::Internet.into(),
        rdata: "test txt".to_string(),
    };
    println!("rec to create: {rec_to_create:?}");
    create_record(&pool, rec_to_create).await?;

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
    load_zone(pool.acquire().await?, zone.clone()).await?;

    let zone_first = get_zone(&pool, zone.clone().name).await?.unwrap();

    zone.rname = "foo.example.com".to_string();
    load_zone(pool.acquire().await?, zone.clone()).await?;

    let zone_second = get_zone(&pool, zone.clone().name).await?.unwrap();

    assert_ne!(zone_first, zone_second);
    // TODO: work out how to compare the full record list

    Ok(())
}

///Hand it a filezone and it'll update the things
pub async fn load_zone(
    mut conn: PoolConnection<Sqlite>,
    zone: FileZone,
) -> Result<(), sqlx::Error> {
    // let mut conn = pool.acquire().await?;

    conn.transaction(|conn| {
        Box::pin(async move {
            // check the zone exists
            let find_zone = match get_zone_with_conn(conn, zone.clone().name).await {
                Ok(val) => {
                    log::trace!("Found existing zone");
                    val
                }
                Err(err) => {
                    // failed to query the DB
                    return Err(err);
                }
            };

            let updated_zone: FileZone = match find_zone {
                None => {
                    // if it's new, add it
                    log::debug!("Creating zone {zone:?}");
                    create_zone_with_conn(conn, zone.clone()).await?;
                    get_zone_with_conn(conn, zone.name.to_owned())
                        .await?
                        .unwrap()
                }
                Some(ez) => {
                    if !zone.matching_data(&ez) {
                        // update it if it's wrong
                        log::debug!("Updating zone");
                        let mut new_zone = zone.clone();
                        new_zone.id = ez.id;
                        log::debug!("Updated: {:?}", update_zone_with_conn(conn, new_zone).await?);
                    }
                    get_zone_with_conn(conn, zone.clone().name)
                        .await
                        .unwrap()
                        .unwrap()
                }
            };
            log::trace!("Zone after update: {updated_zone:?}");

            // drop all the records
            let mut args = SqliteArguments::default();
            args.add(updated_zone.id as f64);

            log::debug!("Dropping all records for zone {zone:?}");
            sqlx_core::query::query_with("delete from records where zoneid = ?", args)
                .execute(&mut *conn)
                .await?;

            // add the records
            for record in zone.records {
                log::trace!("Creating new zone record: {record:?}");
                create_record_with_conn(conn, record).await?;
            }

            // done!
            Ok(())
        })
    })
    .await?;

    // todo!()
    Ok(())
}

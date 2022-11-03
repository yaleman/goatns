use std::str::FromStr;
use std::time::Duration;

use crate::config::ConfigFile;
use crate::enums::{RecordClass, RecordType};

use crate::resourcerecord::InternalResourceRecord;
use crate::zones::FileZone;
use crate::zones::FileZoneRecord;
use serde::{Deserialize, Serialize};
use sqlx::pool::PoolConnection;
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions, SqliteRow};
use sqlx::{
    Arguments, ConnectOptions, Connection, Pool, Row, Sqlite, SqliteConnection, SqlitePool,
    Transaction,
};

#[cfg(test)]
mod test;

const SQL_VIEW_RECORDS: &str = "records_merged";

pub async fn get_conn(config: &ConfigFile) -> Result<Pool<Sqlite>, String> {
    let db_path: &str = &shellexpand::full(&config.sqlite_path).unwrap();
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    log::debug!("Opening Database: {db_url}");

    let mut options = match SqliteConnectOptions::from_str(&db_url) {
        Ok(value) => value,
        Err(error) => return Err(format!("connection failed: {error:?}")),
    };
    options.log_statements(log::LevelFilter::Trace);
    // log anything that takes longer than 1s
    // TODO: make this configurable
    options.log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(1));

    match SqlitePool::connect_with(options).await {
        Ok(value) => Ok(value),
        Err(err) => Err(format!("Error opening SQLite DB ({db_url:?}): {err:?}")),
    }
}

/// Do the basic setup and checks (if we write any)
pub async fn start_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    create_zones_table(pool).await?;
    create_users_table(pool).await?;
    create_records_table(pool).await?;
    create_ownership_table(pool).await?;
    log::info!("Completed DB Startup!");
    Ok(())
}

pub async fn create_users_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    log::info!("Ensuring DB Users table exists");
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS
        users (
            id  INTEGER PRIMARY KEY,
            username TEXT NOT NULL,
            email TEXT NOT NULL,
            disabled BOOL NOT NULL
        )"#,
    )
    .execute(&mut pool.acquire().await?)
    .await?;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS
        ind_users_fields
        ON users ( username, email )",
    )
    .execute(&mut pool.acquire().await?)
    .await?;

    Ok(())
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct User {
    #[serde(default)]
    pub id: u64,
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub owned_zones: Vec<u64>,
}

impl User {
    #[allow(dead_code, unused_variables)]
    pub async fn create(&self, pool: &SqlitePool) -> Result<usize, sqlx::Error> {
        // TODO: test user create
        let res = sqlx::query("INSERT into users (username, email, disabled) VALUES(?, ?, ?)")
            .bind(&self.username)
            .bind(&self.email)
            .bind(false)
            .execute(&mut pool.acquire().await?)
            .await?;

        Ok(1)
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // TODO: test user delete
        let mut txn = pool.begin().await?;

        let res = sqlx::query("DELETE FROM ownership WHERE userid = ?")
            .bind(self.id as f64)
            .execute(&mut *txn)
            .await?;

        let res = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(self.id as f64)
            .execute(&mut *txn)
            .await?;

        txn.commit().await?;

        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ZoneOwnership {
    #[serde(default)]
    id: u64,
    pub userid: u64,
    pub zoneid: u64,
}

impl ZoneOwnership {
    #[allow(dead_code, unused_variables)]
    pub async fn create(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // TODO: test ownership create
        sqlx::query(
            "INSERT INTO ownership (zoneid, userid) VALUES ( ?, ? ) ON CONFLICT DO NOTHING",
        )
        .bind(self.zoneid as f64)
        .bind(self.userid as f64)
        .execute(&mut pool.acquire().await?)
        .await?;
        Ok(())
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete(&self, pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        // TODO: test ownership delete
        let res = sqlx::query("DELETE FROM ownership WHERE zoneid = ? AND userid = ?")
            .bind(self.zoneid as f64)
            .bind(self.userid as f64)
            .execute(&mut pool.acquire().await?)
            .await?;
        Ok(res.rows_affected())
    }
    #[allow(dead_code, unused_variables)]
    pub async fn delete_for_user(self, pool: &SqlitePool) -> Result<User, sqlx::Error> {
        // TODO: test user delete
        // TODO: delete all ownership records
        todo!();
    }
}

pub async fn create_ownership_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await.unwrap();

    #[cfg(test)]
    eprintln!("Ensuring DB Ownership table exists");
    log::info!("Ensuring DB Ownership table exists");
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS
        ownership (
            id   INTEGER PRIMARY KEY,
            zoneid INTEGER NOT NULL,
            userid INTEGER NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id),
            FOREIGN KEY(userid) REFERENCES users(id)
        )"#,
    )
    .execute(&mut tx)
    .await?;

    #[cfg(test)]
    eprintln!("Ensuring DB Ownership index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_ownership
        ON ownership (
            zoneid,
            userid
        )",
    )
    .execute(&mut tx)
    .await?;

    tx.commit().await
}

pub async fn create_zones_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await.unwrap();

    log::info!("Ensuring DB Zones table exists");
    sqlx::query(
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
    .await?;

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

pub async fn create_records_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    log::info!("Ensuring DB Records table exists");

    let mut tx = pool.begin().await.unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS
        records (
            id      INTEGER PRIMARY KEY,
            zoneid  INTEGER NOT NULL,
            name    TEXT, /* this can be null for apex records */
            ttl     INTEGER,
            rrtype   INTEGER NOT NULL,
            rclass  INTEGER NOT NULL,
            rdata   TEXT NOT NULL,
            FOREIGN KEY(zoneid) REFERENCES zones(id)
        )",
    )
    .execute(&mut tx)
    .await?;
    log::info!("Ensuring DB Records index exists");
    sqlx::query(
        "CREATE UNIQUE INDEX
        IF NOT EXISTS
        ind_records
        ON records (
            id,zoneid,name,rrtype,rclass
        )",
    )
    .execute(&mut tx)
    .await?;
    log::info!("Ensuring DB Records view exists");
    // this view lets us query based on the full name
    sqlx::query(
        &format!("CREATE VIEW IF NOT EXISTS {} ( record_id, zone_id, rrtype, rclass, rdata, name, ttl ) as
        SELECT records.id as record_id, zones.id as zone_id, records.rrtype, records.rclass ,records.rdata,
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

/// create a resource record within a zone
pub async fn create_record_with_conn(
    txn: &mut Transaction<'_, Sqlite>,
    record: FileZoneRecord,
) -> Result<u64, sqlx::Error> {
    let rclass: u16 = record.class as u16;
    let rrtype = RecordType::from(record.rrtype);
    let rrtype = rrtype as u16;

    let record_name = match record.name.len() {
        0 => None,
        _ => Some(record.name),
    };

    let mut args = SqliteArguments::default();
    let input_args: Vec<Option<String>> = vec![
        Some(record.zoneid.to_string()),
        record_name,
        Some(record.ttl.to_string()),
        Some(rrtype.to_string()),
        Some(rclass.to_string()),
        Some(record.rdata),
    ];
    for arg in input_args {
        args.add(arg);
    }
    let result = sqlx::query_with(
        "INSERT INTO records (zoneid, name, ttl,rrtype, rclass, rdata)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        args,
    )
    .execute(&mut *txn)
    .await?;
    Ok(result.rows_affected())
}

/// Query the zones table, name_or_id can be the zoneid or the name - if they match then you're bad and you should feel bacd.
pub async fn get_zone_with_txn(
    txn: &mut Transaction<'_, Sqlite>,
    name_or_id: &str,
) -> Result<Option<FileZone>, sqlx::Error> {
    // let mut args = SqliteArguments::default();
    // args.add(name);

    let result = sqlx::query(
        "SELECT
        id, name, rname, serial, refresh, retry, expire, minimum
        FROM zones
        WHERE name = ? or id = ? LIMIT 1",
    )
    .bind(name_or_id)
    .bind(name_or_id)
    .fetch_optional(&mut *txn)
    .await?;
    let mut zone = match result {
        None => return Ok(None),
        Some(row) => {
            let id: i64 = row.get(0);

            FileZone {
                id: id as u64,
                name: row.get(1),
                rname: row.get(2),
                serial: row.get(3),
                refresh: row.get(4),
                retry: row.get(5),
                expire: row.get(6),
                minimum: row.get(7),
                records: vec![],
            }
        }
    };

    let result = sqlx::query(
        "SELECT
        id, zoneid, name, ttl, rrtype, rclass, rdata
        FROM records
        WHERE zoneid = ?",
    )
    .bind(zone.id as f64)
    .fetch_all(&mut *txn)
    .await?;

    zone.records = result
        .into_iter()
        .filter_map(|r| match FileZoneRecord::try_from(r) {
            Ok(val) => Some(val),
            Err(_) => None,
        })
        .collect();
    Ok(Some(zone))
}

pub async fn get_zone(pool: &SqlitePool, name: String) -> Result<Option<FileZone>, sqlx::Error> {
    let mut txn = pool.begin().await?;

    get_zone_with_txn(&mut txn, &name).await
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

impl TryFrom<SqliteRow> for InternalResourceRecord {
    type Error = String;
    fn try_from(row: SqliteRow) -> Result<InternalResourceRecord, String> {
        let record_id: i64 = row.get(0);
        let zoneid: i64 = row.get(1);
        let zoneid: u64 = zoneid.try_into().unwrap_or(0);
        let record_name: String = row.get(2);
        let record_class: u16 = row.get(3);
        let record_type: u16 = row.get(4);
        let rrtype: &str = RecordType::from(&record_type).into();
        let rdata: String = row.get(5);
        let ttl: u32 = row.get(6);
        InternalResourceRecord::try_from(FileZoneRecord {
            name: record_name,
            ttl,
            zoneid,
            id: record_id as u64,
            rrtype: rrtype.to_string(),
            class: RecordClass::from(&record_class),
            rdata,
        })
    }
}

pub async fn get_records(
    conn: &Pool<Sqlite>,
    name: String,
    rrtype: RecordType,
    rclass: RecordClass,
) -> Result<Vec<InternalResourceRecord>, sqlx::Error> {
    let res = sqlx::query(&format!(
        "SELECT
        record_id, zone_id, name, rclass, rrtype, rdata, ttl
        FROM {}
        WHERE name = ? AND rrtype = ? AND rclass = ?",
        SQL_VIEW_RECORDS
    ))
    .bind(&name)
    .bind((rrtype as u16).to_string())
    .bind((rclass as u16).to_string())
    .fetch_all(&mut conn.acquire().await?)
    .await?;

    if res.is_empty() {
        log::trace!("No results returned for {name}");
    }

    let mut results: Vec<InternalResourceRecord> = vec![];
    for row in res {
        if let Ok(irr) = InternalResourceRecord::try_from(row) {
            results.push(irr);
        }
    }

    log::trace!("results: {results:?}");
    Ok(results)
}

pub async fn get_zone_records(
    txn: &mut Transaction<'_, Sqlite>,
    zone_id: u64,
) -> Result<Vec<FileZoneRecord>, sqlx::Error> {
    let res = sqlx::query(
        "SELECT
        id, zoneid, name, ttl, rrtype, rclass, rdata
        FROM records
        WHERE zoneid = ?",
    )
    .bind(&zone_id.to_string())
    .fetch_all(&mut *txn)
    .await?;

    if res.is_empty() {
        log::trace!("No results returned for zone_id={zone_id}");
    }

    let mut results: Vec<FileZoneRecord> = vec![];
    for row in res {
        if let Ok(irr) = FileZoneRecord::try_from(row) {
            results.push(irr);
        }
    }

    log::trace!("results: {results:?}");
    Ok(results)
}

#[cfg(test)]
/// create a zone example.com
async fn test_create_example_com_zone(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    create_zone(&pool, test_example_com_zone()).await?;
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
        create_record(
            &pool,
            FileZoneRecord {
                zoneid,
                name,
                rrtype: RecordType::A.to_string(),
                class: RecordClass::Internet,
                rdata,
                id: i as u64,
                ttl: i as u32,
            },
        )
        .await?;
    }
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

    let records = get_zone_records(&mut pool.begin().await?, zone.id).await?;
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
        zoneid,
        ttl: 123,
        id: 1,
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
        id: 1,
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

pub async fn load_zone_with_txn(
    txn: &mut Transaction<'_, Sqlite>,
    zone: &FileZone,
) -> Result<(), sqlx::Error> {
    // check the zone exists
    let find_zone = match get_zone_with_txn(txn, &zone.name).await {
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
            #[cfg(test)]
            eprintln!("Creating zone {zone:?}");
            log::debug!("Creating zone {zone:?}");
            create_zone_with_conn(txn, zone.clone()).await?;

            #[cfg(test)]
            eprintln!("Done creating zone");
            get_zone_with_txn(txn, &zone.name).await?.unwrap()
        }
        Some(ez) => {
            if !zone.matching_data(&ez) {
                // update it if it's wrong

                #[cfg(test)]
                eprintln!("Updating zone");
                log::debug!("Updating zone");
                let mut new_zone = zone.clone();
                new_zone.id = ez.id;

                let updated = update_zone_with_conn(txn, new_zone).await?;
                #[cfg(test)]
                eprintln!("Updated: {:?} record", updated);
                log::debug!("Updated: {:?} record", updated);
            }
            get_zone_with_txn(txn, &zone.name).await.unwrap().unwrap()
        }
    };
    #[cfg(test)]
    eprintln!("Zone after update: {updated_zone:?}");
    log::trace!("Zone after update: {updated_zone:?}");

    // drop all the records
    let mut args = SqliteArguments::default();
    args.add(updated_zone.id as f64);

    #[cfg(test)]
    eprintln!("Dropping all records for zone {zone:?}");
    log::debug!("Dropping all records for zone {zone:?}");
    sqlx_core::query::query_with("delete from records where zoneid = ?", args)
        .execute(&mut *txn)
        .await?;

    // add the records
    for mut record in zone.records.clone() {
        #[cfg(test)]
        eprintln!("Creating new zone record: {record:?}");
        log::trace!("Creating new zone record: {record:?}");
        record.zoneid = updated_zone.id;
        if record.name == "@" {
            record.name = "".to_string();
        }
        create_record_with_conn(txn, record).await?;
    }
    Ok(())
}

///Hand it a filezone and it'll update the things
pub async fn load_zone(
    mut conn: PoolConnection<Sqlite>,
    zone: FileZone,
) -> Result<(), sqlx::Error> {
    let res: Result<(), sqlx::Error> = conn
        .transaction(|txn| {
            Box::pin(async move {
                load_zone_with_txn(txn, &zone).await?;
                // done!
                Ok(())
            })
        })
        .await;

    if let Err(err) = res {
        eprintln!("Error loading zone: {err:?}");
    };
    Ok(())
}

/// export a zone!
pub async fn export_zone(
    mut conn: PoolConnection<Sqlite>,
    zone_id: u64,
) -> Result<FileZone, sqlx::Error> {
    let mut txn = conn.begin().await?;

    let mut zone = match get_zone_with_txn(&mut txn, &zone_id.to_string()).await? {
        None => {
            eprintln!("Couldn't find zone with id: {zone_id}");
            return Err(sqlx::Error::RowNotFound);
        }
        Some(value) => value,
    };

    let zone_records = get_zone_records(&mut txn, zone.id).await?;

    zone.records = zone_records;

    Ok(zone)
}

pub async fn export_zone_json(
    conn: PoolConnection<Sqlite>,
    zone_id: u64,
) -> Result<String, sqlx::Error> {
    let zone = export_zone(conn, zone_id).await?;

    match serde_json::to_string(&zone) {
        Ok(value) => Ok(value),
        Err(err) => Err(sqlx::Error::Protocol(format!("{err:?}"))),
    }
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
            eprintln!("couldn't find zone {}", testzone.name);
            return Err(sqlx::Error::RowNotFound);
        }
    };

    let records_to_create = 100usize;
    eprintln!("Creating records");
    test_create_example_com_records(&pool, zone.id, records_to_create).await?;

    eprintln!("Exporting zone {}", zone.id);
    let exported_zone = export_zone(pool.acquire().await?, zone.id).await?;
    eprintln!("Done exporting zone");

    println!("found {} records", exported_zone.records.len());
    assert_eq!(exported_zone.records.len(), records_to_create);

    let json_result = serde_json::to_string(&exported_zone).unwrap();

    println!("{json_result}");

    let export_json_result = export_zone_json(pool.acquire().await?, zone.id).await?;

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
    load_zone(pool.acquire().await?, example_zone.clone()).await?;
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

    let _res = export_zone_json(pool.acquire().await?, 1).await?;

    Ok(())
}

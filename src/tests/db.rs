use chrono::{TimeDelta, Utc};

use crate::db::test::test_get_sqlite_memory;
use crate::db::{cron_db_cleanup, get_zones_with_txn, start_db, DBEntity, ZoneOwnership};
use crate::tests::test_harness;

#[test]
fn zoneownership_serde() {
    let test_str = r#"{"id":1,"userid":1,"zoneid":1}"#;

    let zo: ZoneOwnership = serde_json::from_str(test_str).unwrap();
    assert_eq!(zo.id, Some(1));

    let test_str = r#"{"userid":1,"zoneid":1}"#;
    let zo: ZoneOwnership = serde_json::from_str(test_str).unwrap();
    assert_eq!(zo.id, None);

    let res = serde_json::to_string(&zo).unwrap();

    assert_eq!(res, test_str);
}
#[tokio::test]
async fn userauthtoken_saves() -> Result<(), sqlx::Error> {
    use crate::db::UserAuthToken;

    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await.unwrap();

    test_harness::create_test_user(&pool).await?;

    println!("Creating UAT Object");

    let uat = UserAuthToken {
        name: "Test Token".to_string(),
        id: None,
        issued: Utc::now(),
        expiry: None,
        userid: 1,
        tokenkey: "tokenkey".to_string(),
        tokenhash: "hello world".to_string(),
    };
    println!("Saving UAT Object to DB: {uat:?}");

    uat.save(&pool).await?;

    println!("Saving duplicate UAT Object to DB: {uat:?}");
    uat.save(&pool)
        .await
        .expect_err("Creating a duplicate token value should fail!");
    println!("Saving duplicate UAT Object to DB: {uat:?}");
    uat.save(&pool)
        .await
        .expect_err("Creating a duplicate token value should fail!");

    println!("Done!");

    Ok(())
}

#[tokio::test]
async fn userauthtoken_expiry() -> Result<(), sqlx::Error> {
    use crate::db::UserAuthToken;

    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await.unwrap();

    test_harness::create_test_user(&pool).await?;

    println!("Creating UAT Objects");
    let tokenhash = "hello world".to_string();
    #[allow(clippy::expect_used)]
    let expiry = Utc::now() - TimeDelta::try_hours(60).expect("how did this fail?");
    let uat = UserAuthToken {
        id: None,
        name: "Test Token".to_string(),
        issued: Utc::now(),
        expiry: Some(expiry),
        userid: 1,
        tokenkey: "hello world".to_string(),
        tokenhash,
    };
    println!("Saving UAT Object 1 to DB: {uat:?}");

    uat.save(&pool).await?;
    let tokenhash = "hello world this should exist".to_string();
    #[allow(clippy::expect_used)]
    let expiry = Utc::now() + TimeDelta::try_hours(60).expect("how did this fail?");
    let uat = UserAuthToken {
        id: None,
        name: "Test Token".to_string(),
        issued: Utc::now(),
        expiry: Some(expiry),
        userid: 1,
        tokenkey: "hello world".to_string(),
        tokenhash,
    };
    println!("Saving UAT Object 2 to DB: {uat:?}");
    let res = uat.save(&pool).await;
    println!("result: {res:?}");

    print!("Starting DB Cleanup... ");
    UserAuthToken::cleanup(&pool).await?;
    println!("Done!");

    match UserAuthToken::get(&pool, 1).await {
        Ok(uat) => panic!("We shouldn't find this! {uat:?}"),
        Err(err) => println!("Didn't find the UserAuthToken after cleanup, is good. Got {err:?}"),
    };

    assert!(UserAuthToken::get(&pool, 2).await.is_ok());

    println!("Done!");

    Ok(())
}

#[tokio::test]
async fn test_cron_db_cleanup() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await.unwrap();

    test_harness::create_test_user(&pool).await?;
    println!("doing cleanup");
    cron_db_cleanup(pool, core::time::Duration::from_micros(100), Some(2)).await;
    println!("done with cleanup cycle");

    Ok(())
}

#[tokio::test]
async fn testget_zones_with_txn() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await.unwrap();

    test_harness::create_test_user(&pool).await?;

    let mut txn = pool.begin().await?;
    let zones = get_zones_with_txn(&mut txn, 0, 10).await?;
    drop(txn);

    assert!(zones.is_empty());

    test_harness::import_test_zone_file(&pool).await.unwrap();

    let mut txn = pool.begin().await?;
    let zones = get_zones_with_txn(&mut txn, 100, 0).await?;

    assert!(!zones.is_empty());

    Ok(())
}

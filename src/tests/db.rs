use chrono::Utc;

use crate::db::test::test_get_sqlite_memory;
use crate::db::{start_db, DBEntity, ZoneOwnership};
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
    let tokenhash = "hello world".to_string();
    let uat = UserAuthToken {
        name: "Test Token".to_string(),
        id: None,
        issued: Utc::now(),
        expiry: None,
        userid: 1,
        tokenhash,
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
    let expiry = Utc::now() - chrono::Duration::hours(60);
    let uat = UserAuthToken {
        id: Some(5),
        name: "Test Token".to_string(),
        issued: Utc::now(),
        expiry: Some(expiry),
        userid: 1,
        tokenhash,
    };
    println!("Saving UAT Object to DB: {uat:?}");

    uat.save(&pool).await?;
    let tokenhash = "hello world this should exist".to_string();
    let expiry = Utc::now() + chrono::Duration::hours(60);
    let uat = UserAuthToken {
        id: Some(5),
        name: "Test Token".to_string(),
        issued: Utc::now(),
        expiry: Some(expiry),
        userid: 1,
        tokenhash,
    };
    println!("Saving UAT Object to DB: {uat:?}");

    uat.save(&pool).await?;

    print!("Starting DB Cleanup... ");
    UserAuthToken::cleanup(&pool).await?;
    println!("Done!");

    match UserAuthToken::get(&pool, 1).await {
        Ok(uat) => panic!("We shouldn't find this! {uat:?}"),
        Err(err) => println!("Didn't find the uat after cleanup, is gud. Got {err:?}"),
    };

    assert!(UserAuthToken::get(&pool, 2).await.is_ok());

    println!("Done!");

    Ok(())
}

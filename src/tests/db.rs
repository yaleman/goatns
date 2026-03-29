use super::prelude::*;

use chrono::{TimeDelta, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, PaginatorTrait, TransactionTrait};

use crate::db::{cron_db_cleanup, entities};
use crate::tests::test_harness::{self, create_test_user};
use crate::web::utils::generate_token_key;

#[tokio::test]
async fn userauthtoken_saves() -> Result<(), GoatNsError> {
    let dbconn = test_get_sqlite_memory().await;

    let user = create_test_user(&dbconn).await;

    println!("Creating UAT Object");

    let uat = entities::user_tokens::ActiveModel {
        name: Set("Test Token".to_string()),
        id: NotSet,
        issued: Set(Utc::now()),
        expiry: Set(Some(Utc::now() + TimeDelta::days(1))),
        userid: Set(user.id),
        key: Set(generate_token_key()),
        hash: Set("hello world".to_string()),
    };
    println!("Saving UAT Object to DB: {uat:?}");

    let uat = uat.insert(&dbconn).await?;

    println!("Saving duplicate UAT Object to DB: {uat:?}");
    let uat2 = uat.clone().into_active_model();
    uat2.insert(&dbconn)
        .await
        .expect_err("Creating a duplicate token value should fail!");

    println!("Saving duplicate UAT Object to DB: {uat:?}");
    let uat2 = uat.clone().into_active_model();
    uat2.insert(&dbconn)
        .await
        .expect_err("Creating a duplicate token value should fail!");

    println!("Done!");

    Ok(())
}
#[tokio::test]
async fn userauthtoken_expiry() -> Result<(), GoatNsError> {
    let dbconn = test_get_sqlite_memory().await;

    let user = test_harness::create_test_user(&dbconn).await;

    println!("Creating UAT Objects");
    let tokenhash = "hello world".to_string();
    let expiry = Utc::now() - TimeDelta::try_hours(60).expect("how did this fail?");
    let uat = entities::user_tokens::ActiveModel {
        id: NotSet,
        name: Set("Test Token".to_string()),
        issued: Set(Utc::now()),
        expiry: Set(Some(expiry)),
        userid: Set(user.id),
        key: Set(generate_token_key()),
        hash: Set(tokenhash),
    };
    println!("Saving UAT Object 1 to DB: {uat:?}");

    uat.insert(&dbconn).await.expect("Failed to save token");
    let tokenhash = "hello world this should exist".to_string();
    let expiry = Utc::now() + TimeDelta::try_hours(60).expect("how did this fail?");
    let uat = entities::user_tokens::ActiveModel {
        id: NotSet,
        name: Set("Test Token".to_string()),
        issued: Set(Utc::now()),
        expiry: Set(Some(expiry)),
        userid: Set(user.id),
        key: Set(generate_token_key()),
        hash: Set(tokenhash),
    };
    println!("Saving UAT Object 2 to DB: {uat:?}");
    let _res = uat
        .insert(&dbconn)
        .await
        .expect("Failed to insert second object");

    print!("Starting DB Cleanup... ");
    entities::user_tokens::Entity::cleanup(&dbconn).await?;
    println!("Done!");

    match entities::user_tokens::Entity::find_by_id(1i64)
        .one(&dbconn)
        .await?
    {
        Some(uat) => panic!("We shouldn't find this! {uat:?}"),
        None => println!("Didn't find the UserAuthToken after cleanup, is good."),
    };

    assert!(
        entities::user_tokens::Entity::find_by_id(2i64)
            .one(&dbconn)
            .await?
            .is_some()
    );

    println!("Done!");

    Ok(())
}

#[tokio::test]
async fn test_cron_db_cleanup() -> Result<(), GoatNsError> {
    let dbconn = test_get_sqlite_memory().await;

    test_harness::create_test_user(&dbconn).await;
    println!("doing cleanup");
    cron_db_cleanup(dbconn, core::time::Duration::from_micros(100), Some(2)).await;
    println!("done with cleanup cycle");

    Ok(())
}

#[tokio::test]
async fn testget_zones_with_txn() -> Result<(), GoatNsError> {
    let dbconn = test_get_sqlite_memory().await;

    test_harness::create_test_user(&dbconn).await;

    let txn = dbconn.begin().await?;
    let zones = entities::zones::Entity::find()
        .count(&txn)
        .await
        .map_err(|e| GoatNsError::Generic(format!("Failed to get zones from database: {e:?}")))?;
    drop(txn);

    assert_eq!(zones, 0);

    test_harness::import_test_zone_file(&dbconn).await?;

    let txn = dbconn.begin().await?;
    let zones = entities::zones::Entity::find()
        .count(&txn)
        .await
        .map_err(|e| GoatNsError::Generic(format!("Failed to get zones from database: {e:?}")))?;
    drop(txn);

    assert!(zones > 0);

    Ok(())
}

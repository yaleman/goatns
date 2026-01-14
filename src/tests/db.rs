use chrono::{TimeDelta, Utc};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, PaginatorTrait, TransactionTrait};
use uuid::Uuid;

use crate::db::test::test_get_sqlite_memory;
use crate::db::{cron_db_cleanup, entities, start_db};
use crate::error::GoatNsError;
use crate::tests::test_harness::{self, create_test_user};
use crate::web::utils::generate_token_key;

#[tokio::test]
async fn userauthtoken_saves() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await?;

    let user = create_test_user(&pool).await;

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

    let uat = uat.insert(&pool).await?;

    println!("Saving duplicate UAT Object to DB: {uat:?}");
    let uat2 = uat.clone().into_active_model();
    uat2.insert(&pool)
        .await
        .expect_err("Creating a duplicate token value should fail!");

    println!("Saving duplicate UAT Object to DB: {uat:?}");
    let uat2 = uat.clone().into_active_model();
    uat2.save(&pool)
        .await
        .expect_err("Creating a duplicate token value should fail!");

    println!("Done!");

    Ok(())
}
#[tokio::test]
async fn userauthtoken_expiry() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await?;

    test_harness::create_test_user(&pool).await;

    println!("Creating UAT Objects");
    let tokenhash = "hello world".to_string();
    #[allow(clippy::expect_used)]
    let expiry = Utc::now() - TimeDelta::try_hours(60).expect("how did this fail?");
    let uat = entities::user_tokens::ActiveModel {
        id: NotSet,
        name: Set("Test Token".to_string()),
        issued: Set(Utc::now()),
        expiry: Set(Some(expiry)),
        userid: Set(Uuid::new_v4()),
        key: Set(generate_token_key()),
        hash: Set(tokenhash),
    };
    println!("Saving UAT Object 1 to DB: {uat:?}");

    uat.save(&pool).await?;
    let tokenhash = "hello world this should exist".to_string();
    #[allow(clippy::expect_used)]
    let expiry = Utc::now() + TimeDelta::try_hours(60).expect("how did this fail?");
    let user_1 = Uuid::new_v4();
    let uat = entities::user_tokens::ActiveModel {
        id: NotSet,
        name: Set("Test Token".to_string()),
        issued: Set(Utc::now()),
        expiry: Set(Some(expiry)),
        userid: Set(user_1),
        key: Set(generate_token_key()),
        hash: Set(tokenhash),
    };
    println!("Saving UAT Object 2 to DB: {uat:?}");
    let res = uat.insert(&pool).await;
    println!("result: {res:?}");

    print!("Starting DB Cleanup... ");
    entities::user_tokens::Entity::cleanup(&pool).await;
    println!("Done!");

    match entities::user_tokens::Entity::find_by_id(1u64)
        .one(&pool)
        .await?
    {
        Some(uat) => panic!("We shouldn't find this! {uat:?}"),
        None => println!("Didn't find the UserAuthToken after cleanup, is good."),
    };

    assert!(
        entities::user_tokens::Entity::find_by_id(2u64)
            .one(&pool)
            .await?
            .is_some()
    );

    println!("Done!");

    Ok(())
}

#[tokio::test]
async fn test_cron_db_cleanup() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await?;

    test_harness::create_test_user(&pool).await;
    println!("doing cleanup");
    cron_db_cleanup(pool, core::time::Duration::from_micros(100), Some(2)).await;
    println!("done with cleanup cycle");

    Ok(())
}

#[tokio::test]
async fn testget_zones_with_txn() -> Result<(), GoatNsError> {
    let pool = test_get_sqlite_memory().await;

    println!("Starting DB");
    start_db(&pool).await?;

    test_harness::create_test_user(&pool).await;

    let mut txn = pool.begin().await?;
    let zones = entities::zones::Entity::find()
        .count(&mut txn)
        .await
        .map_err(|e| GoatNsError::Generic(format!("Failed to get zones from database: {e:?}")))?;
    drop(txn);

    assert_eq!(zones, 0);

    test_harness::import_test_zone_file(&pool).await?;

    let mut txn = pool.begin().await?;
    let zones = entities::zones::Entity::find()
        .count(&mut txn)
        .await
        .map_err(|e| GoatNsError::Generic(format!("Failed to get zones from database: {e:?}")))?;
    drop(txn);

    assert!(zones > 0);

    Ok(())
}

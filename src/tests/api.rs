// use crate::db::test::test_get_sqlite_memory;
// use crate::db::{start_db, DBEntity};
// // use crate::tests::test_harness;
// use crate::zones::FileZone;

// #[tokio::test]
// async fn test_zone_create() -> Result<(),sqlx::Error> {
//     let pool = test_get_sqlite_memory().await;

//     start_db(&pool).await? ;

//     let zone = FileZone{
//         name: "example.goat".to_string(),
//         ..FileZone::default()
//     };

//     zone.save(&pool).await?;

//     zone.delete(&pool).await?;

//     Ok(())
// }

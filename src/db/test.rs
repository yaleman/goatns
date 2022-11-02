use super::{start_db, test_get_sqlite_memory, User};

#[tokio::test]
async fn test_create_user() -> Result<(), sqlx::Error> {
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await?;

    let user = User {
        username: "yaleman".to_string(),
        email: "billy@hello.goat".to_string(),
        owned_zones: vec![],
        ..User::default()
    };

    user.create(&pool).await?;

    let res = user.clone().create(&pool).await;
    assert!(res.is_err());

    Ok(())
}

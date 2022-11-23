use concread::cowcell::asynch::CowCell;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::net::TcpStream;
// use url::Url;

use crate::config::ConfigFile;
use crate::db::test::test_get_sqlite_memory;
use crate::db::{start_db, DBEntity, User, UserAuthToken};
use crate::servers::{self, Servers};
// use crate::tests::utils::wait_for_server;
use crate::web::utils::{create_api_token, ApiToken};
use crate::zones::FileZone;
// use crate::tests::test_harness;
// use crate::zones::FileZone;

pub async fn is_free_port(port: u16) -> bool {
    // TODO: Refactor to use `Result::is_err` in a future PR
    TcpStream::connect(("127.0.0.1", port)).await.is_err()
}

async fn start_test_server() -> (SqlitePool, Servers, CowCell<ConfigFile>) {
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await.unwrap();

    let config = crate::config::ConfigFile::try_as_cowcell(Some(
        &"./examples/test_config/goatns-test.json".to_string(),
    ))
    .unwrap();

    let mut port: u16 = 9000;

    loop {
        if is_free_port(port).await {
            break;
        }
        port += 1;

        if port > 10000 {
            panic!("Couldn't find a port")
        }
    }

    let mut config_tx = config.write().await;
    config_tx.api_port = port;
    config_tx.commit().await;

    // println!("Starting channels");
    let (agent_sender, datastore_tx, datastore_rx) = crate::utils::start_channels();

    let tcpserver = tokio::spawn(servers::tcp_server(
        config.read().await,
        datastore_tx.clone(),
        agent_sender.clone(),
    ));
    // start all the things!
    let datastore_manager =
        tokio::spawn(crate::datastore::manager(datastore_rx, pool.clone(), None));

    println!("Starting API Server on port {port}");
    let apiserver =
        crate::web::build(datastore_tx.clone(), config.read().await, pool.clone()).await;

    println!("Building server struct");
    (
        pool,
        crate::servers::Servers::build(agent_sender)
            .with_datastore(datastore_manager)
            .with_apiserver(apiserver)
            .with_tcpserver(tcpserver),
        config,
    )
}

async fn insert_test_user(pool: &SqlitePool) -> User {
    let mut user = User {
        id: Some(5),
        displayname: "Example user".to_string(),
        username: "example".to_string(),
        email: "example@hello.goat".to_string(),
        disabled: false,
        authref: Some("zooooom".to_string()),
        admin: true,
    };

    let userid = user.save(&pool).await.unwrap();
    user.id = Some(userid);
    user
}

/// Shoves an API token into the DB for a user
async fn insert_test_user_api_token(pool: &SqlitePool, userid: i64) -> Result<ApiToken, ()> {
    println!("creating test token for user {userid:?}");
    let token = create_api_token("lols".as_bytes(), 900, userid);

    UserAuthToken {
        id: None,
        name: "test token".to_string(),
        issued: token.issued,
        expiry: token.expiry,
        tokenkey: token.token_key.to_owned(),
        tokenhash: token.token_hash.to_owned(),
        userid,
    }
    .save(&pool)
    .await
    .unwrap();

    Ok(token)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_api_zone_create() -> Result<(), sqlx::Error> {
    // here we stand up the servers
    let (pool, servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.unwrap())
        .await
        .unwrap();
    println!("Created token... {token:?}");

    #[derive(Deserialize, Serialize)]
    struct AuthStruct {
        pub tokenkey: String,
        pub token: String,
    }

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .build()
        .unwrap();

    // println!("API Server ID: {}", orly.apiserver.as_ref().unwrap().id());
    // wait_for_server(Url::parse(&format!("https://localhost:{api_port}/status")).unwrap()).await;
    println!("Logging in with the token...");
    // println!("API Server ID: {}", orly.apiserver.as_ref().unwrap().id());
    let res = client
        .post(&format!("https://localhost:{api_port}/api/login"))
        .json(&AuthStruct {
            tokenkey: token.token_key,
            token: token.token_secret.to_owned(),
        })
        .send()
        .await
        .unwrap();
    // println!("API Server ID: {}", orly.apiserver.as_ref().unwrap().id());
    println!("{:?}", res);
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let newzone = FileZone {
        id: 1234,
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        ..Default::default()
    };

    println!("Sending zone create");

    let res = client
        .post(&format!("https://localhost:{api_port}/api/zone"))
        .header("Authorization", format!("Bearer {}", token.token_secret))
        .json(&newzone)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    // println!("API Server ID: {}", orly.apiserver.as_ref().unwrap().id());

    apiserver.abort();
    Ok(())
}

use crate::config::ConfigFile;
use crate::db::test::test_get_sqlite_memory;
use crate::db::{start_db, DBEntity, User, UserAuthToken, ZoneOwnership};
use crate::servers::{self, Servers};
use crate::web::utils::{create_api_token, ApiToken};
use crate::zones::FileZone;
use concread::cowcell::asynch::CowCell;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::net::TcpStream;

pub async fn is_free_port(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).await.is_err()
}

async fn start_test_server() -> (SqlitePool, Servers, CowCell<ConfigFile>) {
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await.unwrap();

    let config = crate::config::ConfigFile::try_as_cowcell(Some(
        &"./examples/test_config/goatns-test.json".to_string(),
    ))
    .unwrap();

    use rand::thread_rng;
    use rand::Rng;
    let mut rng = thread_rng();
    let mut port: u16 = rng.gen_range(2000..=65000);
    loop {
        if is_free_port(port).await {
            break;
        }
        port = rng.gen_range(2000..=65000);
    }

    let mut config_tx = config.write().await;
    config_tx.api_port = port;
    config_tx.commit().await;

    // println!("Starting channels");
    let (agent_sender, datastore_tx, datastore_rx) = crate::utils::start_channels();

    let udpserver = tokio::spawn(servers::udp_server(
        config.read().await,
        datastore_tx.clone(),
        agent_sender.clone(),
    ));
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
            .with_udpserver(udpserver)
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

#[derive(Deserialize, Serialize)]
struct AuthStruct {
    pub tokenkey: String,
    pub token: String,
}

#[tokio::test] //(flavor = "multi_thread", worker_threads = 4)]
async fn test_api_zone_create() -> Result<(), sqlx::Error> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    // let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.unwrap())
        .await
        .unwrap();
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .build()
        .unwrap();

    println!("Logging in with the token...");
    let res = client
        .post(&format!("https://localhost:{api_port}/api/login"))
        .json(&AuthStruct {
            tokenkey: token.token_key,
            token: token.token_secret.to_owned(),
        })
        .send()
        .await
        .unwrap();
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

    // apiserver.abort();
    Ok(())
}

#[tokio::test] //(flavor = "multi_thread", worker_threads = 4)]
async fn test_api_zone_create_delete() -> Result<(), sqlx::Error> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    // let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.unwrap())
        .await
        .unwrap();
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .build()
        .unwrap();

    println!("Logging in with the token...");
    let res = client
        .post(&format!("https://localhost:{api_port}/api/login"))
        .json(&AuthStruct {
            tokenkey: token.token_key,
            token: token.token_secret.to_owned(),
        })
        .send()
        .await
        .unwrap();
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
        .json(&newzone)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let res_content = res.bytes().await;
    println!("content from create: {res_content:?}");

    println!("Sending zone delete");
    let res = client
        .delete(&format!("https://localhost:{api_port}/api/zone/1234"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let res_content = res.bytes().await;
    println!("content from delete: {res_content:?}");

    // apiserver.abort();
    Ok(())
}

#[tokio::test] //(flavor = "multi_thread", worker_threads = 4)]
async fn test_api_zone_create_update() -> Result<(), sqlx::Error> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    // let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.unwrap())
        .await
        .unwrap();
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .build()
        .unwrap();

    println!("Logging in with the token...");
    let res = client
        .post(&format!("https://localhost:{api_port}/api/login"))
        .json(&AuthStruct {
            tokenkey: token.token_key,
            token: token.token_secret.to_owned(),
        })
        .send()
        .await
        .unwrap();
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
    println!("Saving zone");
    newzone.save(&pool).await?;
    println!("Saving zone ownership");
    ZoneOwnership {
        id: None,
        userid: user.id.unwrap(),
        zoneid: newzone.id,
    }
    .save(&pool)
    .await?;

    println!("updating zone rname to steve@example.goat");
    let newzone = FileZone {
        rname: "steve@example.goat".to_string(),
        ..newzone
    };

    println!("Sending zone update");
    let res = client
        .put(&format!(
            "https://localhost:{api_port}/api/zone/{}",
            newzone.id
        ))
        .json(&newzone)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let res_content = res.bytes().await;
    println!("content from patch: {res_content:?}");

    Ok(())
}

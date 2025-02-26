use crate::config::ConfigFile;
use crate::db::test::test_get_sqlite_memory;
use crate::db::zoneownership::ZoneOwnership;

use crate::db::{start_db, DBEntity, User, UserAuthToken};
use crate::enums::RecordType;
use crate::error::GoatNsError;
use crate::servers::{self, Servers};
use crate::web::api::auth::AuthPayload;
use crate::web::utils::{create_api_token, ApiToken};
use crate::zones::{FileZone, FileZoneRecord};
use concread::cowcell::asynch::CowCell;
use sqlx::SqlitePool;
use tokio::net::TcpStream;

pub async fn is_free_port(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).await.is_err()
}

pub async fn start_test_server() -> (SqlitePool, Servers, CowCell<ConfigFile>) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await.expect("failed to start DB");

    let config = crate::config::ConfigFile::try_as_cowcell(Some(
        "./examples/test_config/goatns-test.json".to_string(),
    ))
    .expect("failed to parse test config");

    use rand::Rng;
    let mut rng = rand::rng();
    let mut port: u16 = rng.random_range(2000..=65000);
    loop {
        if is_free_port(port).await {
            break;
        }
        port = rng.random_range(2000..=65000);
    }

    let mut config_tx = config.write().await;
    config_tx.api_port = port;
    config_tx.commit();

    // println!("Starting channels");
    let (agent_sender, datastore_tx, datastore_rx) = crate::utils::start_channels();

    let udpserver = tokio::spawn(servers::udp_server(
        config.read(),
        datastore_tx.clone(),
        agent_sender.clone(),
    ));
    let tcpserver = tokio::spawn(servers::tcp_server(
        config.read(),
        datastore_tx.clone(),
        agent_sender.clone(),
    ));
    // start all the things!
    let datastore_manager = tokio::spawn(crate::datastore::manager(
        datastore_rx,
        "test.goatns.goat".to_string(),
        pool.clone(),
        None,
    ));

    println!("Starting API Server on port {port}");
    let apiserver = crate::web::build(datastore_tx.clone(), config.read(), pool.clone())
        .await
        .expect("Failed to start API server");

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

pub async fn insert_test_user(pool: &SqlitePool) -> Box<User> {
    User {
        id: Some(5),
        displayname: "Example user".to_string(),
        username: "example".to_string(),
        email: "example@hello.goat".to_string(),
        disabled: false,
        authref: Some("zooooom".to_string()),
        admin: true,
    }
    .save(pool)
    .await
    .expect("Failed to save test user")
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
    .save(pool)
    .await
    .expect("Failed to save test token");

    Ok(token)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_zone_create() -> Result<(), GoatNsError> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    // let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("api_zone_create Created user... {user:?}");

    println!("api_zone_create Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.expect("no user id found"))
        .await
        .expect("Failed to insert test user api token");
    println!("api_zone_create Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    println!("api_zone_create Logging in with the token...");
    let res = client
        .post(format!("https://localhost:{api_port}/api/login"))
        .timeout(std::time::Duration::from_secs(5))
        .json(&AuthPayload {
            token_key: token.token_key,
            token_secret: token.token_secret.to_owned(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{:?}", res);
    assert_eq!(res.status(), 200);
    println!("api_zone_create => Token login success!");

    let newzone = FileZone {
        id: Some(1234),
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        ..Default::default()
    };

    println!("api_zone_create Sending zone create");
    let res = client
        .post(format!("https://localhost:{api_port}/api/zone"))
        .header("Authorization", format!("Bearer {}", token.token_secret))
        .json(&newzone)
        .send()
        .await
        .expect("Failed to send create request");
    assert_eq!(res.status(), 200);

    let response_zone: FileZone = res
        .json()
        .await
        .inspect_err(|err| println!("Failed to parse response content: {err:?}"))?;

    assert_eq!(response_zone.name, "example.goat");
    assert_eq!(response_zone.serial, 12345);
    assert_ne!(response_zone.serial, 123456);
    drop(pool);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_zone_create_delete() -> Result<(), sqlx::Error> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    // let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.expect("no user id found"))
        .await
        .expect("Failed to insert test user api token");
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    println!("Logging in with the token...");
    let res = client
        .post(format!("https://localhost:{api_port}/api/login"))
        .timeout(std::time::Duration::from_secs(5))
        .json(&AuthPayload {
            token_key: token.token_key,
            token_secret: token.token_secret.to_owned(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{:?}", res);
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let newzone = FileZone {
        id: Some(1234),
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        ..Default::default()
    };

    println!("Sending zone create");
    let res = client
        .post(format!("https://localhost:{api_port}/api/zone"))
        .json(&newzone)
        .send()
        .await
        .expect("Failed to send create request");

    assert_eq!(res.status(), 200);
    let res_content = res.bytes().await;
    println!("content from create: {res_content:?}");

    println!("Sending zone delete");
    let res = client
        .delete(format!("https://localhost:{api_port}/api/zone/1234"))
        .send()
        .await
        .expect("Failed to send delete request");

    assert_eq!(res.status(), 200);
    let res_content = res.bytes().await;
    println!("content from delete: {res_content:?}");

    drop(pool);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_zone_create_update() -> Result<(), GoatNsError> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    // let apiserver = servers.apiserver.unwrap();

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.expect("no user id found"))
        .await
        .expect("Failed to insert test user api token");
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    println!("Logging in with the token...");
    let res = client
        .post(format!("https://localhost:{api_port}/api/login"))
        .timeout(std::time::Duration::from_secs(5))
        .json(&AuthPayload {
            token_key: token.token_key,
            token_secret: token.token_secret.to_owned(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{:?}", res);
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let newzone = FileZone {
        id: Some(1234),
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
        userid: user.id.expect("No user id"),
        zoneid: newzone.id.expect("No zone id"),
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
        .put(format!("https://localhost:{api_port}/api/zone",))
        .json(&newzone)
        .send()
        .await
        .expect("Failed to send update request");

    assert_eq!(res.status(), 200);
    let res_content = res.bytes().await;
    println!("content from patch: {res_content:?}");

    drop(pool);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_record_create() -> Result<(), GoatNsError> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;
    let api_port = config.read().api_port;
    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");
    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.expect("no user id found"))
        .await
        .expect("Failed to insert test user api token");
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    println!("Logging in with the token...");
    let res = client
        .post(format!("https://localhost:{api_port}/api/login"))
        .timeout(std::time::Duration::from_secs(5))
        .json(&AuthPayload {
            token_key: token.token_key,
            token_secret: token.token_secret.to_owned(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{:?}", res);
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let zone = FileZone {
        id: Some(333),
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        ..Default::default()
    }
    .save(&pool)
    .await
    .expect("Failed to save filezone");

    let zo = ZoneOwnership {
        id: None,
        userid: user.id.expect("no user id found"),
        zoneid: zone.id.expect("Failed to get zone id"),
    };
    println!("ZO: {zo:?}");
    zo.save(&pool).await.expect("Failed to save zone ownership");

    println!("building fzr object");
    let fzr = FileZoneRecord {
        id: Some(3),
        class: crate::enums::RecordClass::Internet,
        name: "doggo".to_string(),
        zoneid: Some(333),
        rrtype: RecordType::A.to_string(),
        ttl: 33,
        rdata: "1.2.3.4".to_string(),
    };
    println!("Sending record create");
    let res = client
        .post(format!("https://localhost:{api_port}/api/record"))
        .header("Authorization", format!("Bearer {}", token.token_secret))
        .json(&fzr)
        .send()
        .await
        .expect("Failed to create record");

    assert_eq!(res.status(), 200);
    let response_record: FileZoneRecord = res
        .json()
        .await
        .inspect_err(|err| eprintln!("Failed to get response content: {err:?}"))?;
    assert_eq!(response_record.name, "doggo");
    drop(pool);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_record_delete() -> Result<(), GoatNsError> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;
    let api_port = config.read().api_port;
    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");
    println!("Creating token for user");
    let token = insert_test_user_api_token(&pool, user.id.expect("no user id found"))
        .await
        .expect("Failed to insert test user api token");
    println!("Created token... {token:?}");

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    println!("Logging in with the token...");
    let res = match client
        .post(format!("https://localhost:{api_port}/api/login"))
        .timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(5))
        .json(&AuthPayload {
            token_key: token.token_key,
            token_secret: token.token_secret.to_owned(),
        })
        .send()
        .await
    {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Failed to send login request: {err:?}");
            return Err(GoatNsError::StartupError(
                "Failed to send login request".to_string(),
            ));
        }
    };
    println!("{:?}", res);
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let zone = FileZone {
        id: Some(333),
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        ..Default::default()
    }
    .save(&pool)
    .await
    .expect("Failed to save filezone");

    let zo = ZoneOwnership {
        id: None,
        userid: user.id.expect("no user id found"),
        zoneid: zone.id.expect("Failed to get zone id"),
    };
    println!("ZO: {zo:?}");
    zo.save(&pool).await.expect("failed to save zone ownership");

    println!("creating fzr object in the database");
    let fzr = FileZoneRecord {
        id: Some(3),
        class: crate::enums::RecordClass::Internet,
        name: "doggo".to_string(),
        zoneid: Some(333),
        rrtype: RecordType::A.to_string(),
        ttl: 33,
        rdata: "1.2.3.4".to_string(),
    }
    .save(&pool)
    .await?;

    println!("{fzr:?}");

    println!("Sending record delete");
    let res = client
        .delete(format!("https://localhost:{api_port}/api/record/3"))
        .header("Authorization", format!("Bearer {}", token.token_secret))
        .send()
        .await
        .expect("Failed to send delete request");

    let status = res.status();
    println!("Response content: {:?}", res.text().await);

    assert_eq!(status, 200);

    drop(pool);
    Ok(())
}

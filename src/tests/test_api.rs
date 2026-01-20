use super::prelude::*;

use crate::config::ConfigFile;
use crate::servers::{self, Servers};
use crate::web::api::auth::AuthPayload;
use crate::web::api::records::RecordForm;
use crate::web::api::zones::ZoneForm;
use crate::web::utils::create_api_token;
use concread::cowcell::asynch::CowCell;
use log::info;
use reqwest::StatusCode;
use sea_orm::EntityTrait;
use tokio::net::{TcpListener, UdpSocket};

pub async fn is_free_port(port: u16) -> bool {
    let addr = ("127.0.0.1", port);
    let tcp_listener = TcpListener::bind(addr).await;
    if tcp_listener.is_err() {
        return false;
    }

    let udp_listener = UdpSocket::bind(addr).await;
    if udp_listener.is_err() {
        return false;
    }

    true
}

pub async fn start_test_server() -> (DatabaseConnection, Servers, CowCell<ConfigFile>) {
    test_logging().await;
    let pool = test_get_sqlite_memory().await;

    let config = crate::config::ConfigFile::try_as_cowcell(Some(
        "./examples/test_config/goatns-test.json".to_string(),
    ))
    .expect("failed to parse test config");

    use rand::Rng;
    let mut rng = rand::rng();
    let mut api_port: u16 = rng.random_range(2000..=65000);
    loop {
        if is_free_port(api_port).await {
            break;
        }
        api_port = rng.random_range(2000..=65000);
    }

    let mut dns_port: u16 = rng.random_range(2000..=65000);
    loop {
        if dns_port != api_port && is_free_port(dns_port).await {
            break;
        }
        dns_port = rng.random_range(2000..=65000);
    }

    let mut config_tx = config.write().await;
    config_tx.api_port = api_port;
    config_tx.port = dns_port;
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
    let datastore_manager = tokio::spawn(crate::datastore::manager(
        datastore_rx,
        "test.goatns.goat".to_string(),
        pool.clone(),
        None,
    ));

    info!("Starting API Server on port {api_port}");
    let (_apiserver_tx, apiserver_rx) = tokio::sync::mpsc::channel(5);

    let apiserver = crate::web::build(
        datastore_tx.clone(),
        apiserver_rx,
        config.read().await,
        pool.clone(),
    )
    .await
    .expect("Failed to start API server");

    println!("Building server struct");
    let res = (
        pool,
        crate::servers::Servers::build(agent_sender)
            .with_apiserver(apiserver)
            .with_datastore(datastore_manager)
            .with_udpserver(udpserver)
            .with_tcpserver(tcpserver)
            .with_datastore_tx(datastore_tx),
        config,
    );
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    res
}

pub async fn insert_test_user(pool: &DatabaseConnection) -> entities::users::Model {
    entities::users::ActiveModel {
        id: Set(Uuid::now_v7()),
        displayname: Set("Example user".to_string()),
        username: Set("example".to_string()),
        email: Set("example@hello.goat".to_string()),
        disabled: Set(false),
        authref: Set(Some("zooooom".to_string())),
        admin: Set(true),
    }
    .insert(pool)
    .await
    .expect("Failed to save test user")
}

/// Shoves an API token into the DB for a user
async fn insert_test_user_api_token(
    db: &DatabaseConnection,
    userid: Uuid,
) -> Result<(entities::user_tokens::Model, String), GoatNsError> {
    println!("creating test token for user {userid:?}");
    let (token_secret, token) = create_api_token("lols".as_bytes(), 900, userid);

    let res = token.insert(db).await.map_err(GoatNsError::from)?;
    Ok((res, token_secret))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_zone_create() -> Result<(), GoatNsError> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;

    let user = insert_test_user(&pool).await;
    println!("api_zone_create Created user... {user:?}");

    println!("api_zone_create Creating token for user");
    let (token, token_secret) = insert_test_user_api_token(&pool, user.id)
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
            token_key: token.key,
            token_secret: token_secret.clone(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{res:?}");
    assert_eq!(res.status(), 200);
    println!("api_zone_create => Token login success!");

    let newzone = ZoneForm {
        id: None,
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        refresh: 1234,
        retry: 1234,
    };

    println!("api_zone_create Sending zone create");
    let res = client
        .post(format!("https://localhost:{api_port}/api/zone"))
        .header("Authorization", format!("Bearer {}", token_secret))
        .json(&newzone)
        .send()
        .await
        .expect("Failed to send create request");
    assert_eq!(res.status(), 200);

    let response_zone: entities::zones::Model = res
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

    let api_port = config.read().await.api_port;

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let (token, token_secret) = insert_test_user_api_token(&pool, user.id)
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
            token_key: token.key,
            token_secret,
        })
        .send()
        .await
        .expect("Failed to log in with token");

    println!("{res:?}");
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");
    let newzone = ZoneForm {
        id: None,
        name: "example.goat".to_string(),
        rname: "bob@example.goat".to_string(),
        serial: 12345,
        expire: 30,
        minimum: 1235,
        refresh: 1111,
        retry: 1234234,
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
    let zone: entities::zones::Model = serde_json::from_slice(
        res_content
            .as_ref()
            .expect("Failed to get response content"),
    )
    .expect("Failed to parse zone from response");

    let url = format!("https://localhost:{api_port}/api/zone/{}", zone.id);
    println!("Sending zone delete to URL: {url}");

    let res = client
        .delete(url)
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

    let api_port = config.read().await.api_port;

    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");

    println!("Creating token for user");
    let (token, token_secret) = insert_test_user_api_token(&pool, user.id)
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
            token_key: token.key,
            token_secret: token_secret.to_owned(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{res:?}");
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let newzone = entities::zones::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set("example.goat".to_string()),
        rname: Set("bob@example.goat".to_string()),
        serial: Set(12345),
        expire: Set(30),
        minimum: Set(1235),
        refresh: Set(1111),
        retry: Set(1234234),
    };
    println!("Saving zone");
    let newzone = newzone.insert(&pool).await?;
    println!("Saving zone ownership");
    let _zo = entities::ownership::ActiveModel {
        id: Set(Uuid::now_v7()),
        userid: Set(user.id),
        zoneid: Set(newzone.id),
    }
    .insert(&pool)
    .await?;
    println!("updating zone rname to steve@example.goat");
    let newzone = ZoneForm {
        rname: "steve@example.goat".to_string(),
        ..newzone.into()
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
    let api_port = config.read().await.api_port;
    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");
    println!("Creating token for user");
    let (token, token_secret) = insert_test_user_api_token(&pool, user.id)
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
            token_key: token.key,
            token_secret: token_secret.clone(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    println!("{res:?}");
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("example.goat".to_string()),
        rname: Set("bob@example.goat".to_string()),
        serial: Set(12345),
        expire: Set(30),
        minimum: Set(1235),
        refresh: Set(1341234),
        retry: Set(123456),
    }
    .insert(&pool)
    .await
    .expect("Failed to save filezone");

    let zo = entities::ownership::ActiveModel {
        id: NotSet,
        userid: Set(user.id),
        zoneid: Set(zone.id),
    };
    println!("ZO: {zo:?}");
    zo.insert(&pool)
        .await
        .expect("Failed to save zone ownership");

    println!("building fzr object");
    let fzr = RecordForm {
        id: None,
        name: "doggo".to_string(),
        zoneid: zone.id,
        rclass: RecordClass::Internet,
        rrtype: RecordType::A,
        ttl: Some(33),
        rdata: "1.2.3.4".to_string(),
    };

    println!("Sending record create");
    let res = client
        .post(format!("https://localhost:{api_port}/api/record"))
        .header("Authorization", format!("Bearer {}", token_secret))
        .json(&fzr)
        .send()
        .await
        .expect("Failed to create record");

    assert_eq!(res.status(), 200);
    let response_record: entities::records::Model = res
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
    let api_port = config.read().await.api_port;
    let user = insert_test_user(&pool).await;
    println!("Created user... {user:?}");
    println!("Creating token for user");
    let (token, token_secret) = insert_test_user_api_token(&pool, user.id)
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
            token_key: token.key,
            token_secret: token_secret.clone(),
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
    println!("{res:?}");
    assert_eq!(res.status(), 200);
    println!("=> Token login success!");

    let zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("example.goat".to_string()),
        rname: Set("bob@example.goat".to_string()),
        serial: Set(12345),
        expire: Set(30),
        minimum: Set(1235),
        refresh: Set(0),
        retry: Set(0),
    }
    .insert(&pool)
    .await
    .expect("Failed to save filezone");

    let zo = entities::ownership::ActiveModel {
        id: NotSet,
        userid: Set(user.id),
        zoneid: Set(zone.id),
    };
    println!("ZO: {zo:?}");
    let _ownership = zo
        .insert(&pool)
        .await
        .expect("failed to save zone ownership");

    println!("creating record object in the database");
    let zone_record = entities::records::ActiveModel {
        id: NotSet,
        rclass: Set(RecordClass::Internet.into()),
        name: Set("doggo".to_string()),
        zoneid: Set(zone.id),
        rrtype: Set(RecordType::A.into()),
        ttl: Set(Some(33)),
        rdata: Set("1.2.3.4".to_string()),
    }
    .insert(&pool)
    .await?;

    println!("Record: {zone_record:?}");

    println!("Sending record delete");
    let res = client
        .delete(format!(
            "https://localhost:{api_port}/api/record/{}",
            zone_record.id
        ))
        .header("Authorization", format!("Bearer {}", token_secret))
        .send()
        .await
        .expect("Failed to send delete request");

    let status = res.status();
    println!("Response content: {:?}", res.text().await);

    assert_eq!(status, 200);

    drop(pool);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_record_delete_forbidden_without_ownership() -> Result<(), GoatNsError> {
    let (pool, _servers, config) = start_test_server().await;
    let api_port = config.read().await.api_port;

    let owner = insert_test_user(&pool).await;
    let other_user = insert_test_user(&pool).await;

    let (_owner_token, _owner_secret) = insert_test_user_api_token(&pool, owner.id)
        .await
        .expect("Failed to insert test owner api token");
    let (other_token, other_secret) = insert_test_user_api_token(&pool, other_user.id)
        .await
        .expect("Failed to insert test user api token");

    let zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("example.goat".to_string()),
        rname: Set("bob@example.goat".to_string()),
        serial: Set(12345),
        expire: Set(30),
        minimum: Set(1235),
        refresh: Set(0),
        retry: Set(0),
    }
    .insert(&pool)
    .await
    .expect("Failed to save zone");

    entities::ownership::ActiveModel {
        id: NotSet,
        userid: Set(owner.id),
        zoneid: Set(zone.id),
    }
    .insert(&pool)
    .await
    .expect("failed to save zone ownership");

    let zone_record = entities::records::ActiveModel {
        id: NotSet,
        rclass: Set(RecordClass::Internet.into()),
        name: Set("doggo".to_string()),
        zoneid: Set(zone.id),
        rrtype: Set(RecordType::A.into()),
        ttl: Set(Some(33)),
        rdata: Set("1.2.3.4".to_string()),
    }
    .insert(&pool)
    .await?;

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .post(format!("https://localhost:{api_port}/api/login"))
        .timeout(std::time::Duration::from_secs(5))
        .json(&AuthPayload {
            token_key: other_token.key,
            token_secret: other_secret.clone(),
        })
        .send()
        .await
        .expect("Failed to log in with token");
    assert_eq!(res.status(), 200);

    let res = client
        .delete(format!(
            "https://localhost:{api_port}/api/record/{}",
            zone_record.id
        ))
        .header("Authorization", format!("Bearer {}", other_secret))
        .send()
        .await
        .expect("Failed to send delete request");

    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    let record = entities::records::Entity::find_by_id(zone_record.id)
        .one(&pool)
        .await?;
    assert!(record.is_some());

    drop(pool);
    Ok(())
}

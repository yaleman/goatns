use crate::db::test::test_example_com_zone;
use crate::db::DBEntity;
use crate::enums::RecordClass;
use crate::tests::test_api::insert_test_user;
use crate::tests::test_api::start_test_server;
use crate::zones::FileZoneRecord;

#[tokio::test]
async fn test_doh_get_json() -> Result<(), ()> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    // let apiserver = servers.apiserver.unwrap();

    let _user = insert_test_user(&pool).await;
    test_example_com_zone().save(&pool).await.unwrap();

    let fzr = FileZoneRecord {
        zoneid: Some(1),
        name: "test".to_string(),
        rrtype: "A".to_string(),
        id: None,
        class: RecordClass::Internet,
        rdata: "1.2.3.4".to_string(),
        ttl: 1,
    }
    .save(&pool)
    .await
    .unwrap();

    eprintln!("FZR result: {fzr:?}");

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Accept", "application/dns-json".parse().unwrap());

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let res = client
        .get(&format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::from_u16(200).unwrap());
    eprintln!("{:?}", res);
    eprintln!("{:?}", res.bytes().await);

    // TODO: finish this
    Ok(())
}

#[tokio::test]
async fn test_doh_ask_raw_accept() -> Result<(), ()> {
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Accept", "application/dns-message".parse().unwrap());

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let res = client
        .get(&format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await
        .unwrap();
    eprintln!("{res:?}");
    assert_eq!(res.status(), reqwest::StatusCode::from_u16(200).unwrap());
    Ok(())
}

#[tokio::test]
async fn test_doh_ask_json_accept() -> Result<(), ()> {
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Accept", "application/dns-json".parse().unwrap());

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let res = client
        .get(&format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await
        .unwrap();
    eprintln!("{res:?}");
    assert_eq!(res.status(), reqwest::StatusCode::from_u16(200).unwrap());
    Ok(())
}

#[tokio::test]
async fn test_doh_ask_wrong_accept() -> Result<(), ()> {
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().api_port;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Accept", "application/cheese".parse().unwrap());

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let res = client
        .get(&format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await
        .unwrap();
    eprintln!("{res:?}");
    assert_eq!(res.status(), reqwest::StatusCode::from_u16(406).unwrap());
    Ok(())
}

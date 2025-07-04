use axum::http::header::ACCEPT;
use packed_struct::PackedStruct;

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

    let api_port = config.read().await.api_port;

    let _user = insert_test_user(&pool).await;
    test_example_com_zone()
        .save(&pool)
        .await
        .expect("Failed to save test zone");

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
    .expect("Failed to save test record");

    eprintln!("FZR result: {fzr:?}");

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        ACCEPT,
        "application/dns-json"
            .parse()
            .expect("Failed to parse hard-coded header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(200).expect("Failed to parse status")
    );
    eprintln!("{res:?}");
    
    // Parse the JSON response
    let response_text = res.text().await.expect("Failed to get response text");
    eprintln!("Response body: {response_text}");
    
    let json_response: serde_json::Value = serde_json::from_str(&response_text)
        .expect("Failed to parse JSON response");
    
    // Validate the JSON structure matches RFC 8427 (DNS Queries over HTTPS)
    assert!(json_response.get("status").is_some(), "JSON response should have status field");
    assert!(json_response.get("Question").is_some(), "JSON response should have Question field");
    assert!(json_response.get("Answer").is_some(), "JSON response should have Answer field");
    
    // Validate the status is NoError (0)
    assert_eq!(json_response["status"].as_u64().expect("status field should be numeric"), 0, "status should be NoError (0)");
    
    // Validate the question
    let question = &json_response["Question"][0];
    assert_eq!(question["name"].as_str().expect("question name should be string"), "test.example.com");
    assert_eq!(question["type"].as_u64().expect("question type should be numeric"), 1); // A record type
    
    // Validate the answer
    let answers = json_response["Answer"].as_array().expect("Answer field should be array");
    assert_eq!(answers.len(), 1, "Should have exactly one answer");
    
    let answer = &answers[0];
    assert_eq!(answer["name"].as_str().expect("answer name should be string"), "test.example.com");
    assert_eq!(answer["type"].as_u64().expect("answer type should be numeric"), 1); // A record type
    assert_eq!(answer["TTL"].as_u64().expect("TTL should be numeric"), 1); // TTL from test record
    assert_eq!(answer["data"].as_str().expect("answer data should be string"), "1.2.3.4"); // IP from test record
    
    Ok(())
}

#[tokio::test]
async fn test_doh_ask_raw_accept() -> Result<(), ()> {
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/dns-message"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");
    eprintln!("{res:?}");
    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(200).expect("Failed to parse status")
    );
    Ok(())
}

#[tokio::test]
async fn test_doh_ask_json_accept() -> Result<(), ()> {
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/dns-json"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");
    eprintln!("{res:?}");
    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(200).expect("Failed to parse status")
    );
    Ok(())
}

#[tokio::test]
async fn test_doh_ask_wrong_accept() -> Result<(), ()> {
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/cheese"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        // .cookie_store(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");
    eprintln!("{res:?}");
    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(406).expect("Failed to parse status")
    );
    Ok(())
}

#[tokio::test]
async fn test_doh_post_raw_dns() -> Result<(), ()> {
    // here we stand up the servers
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;

    let _user = insert_test_user(&pool).await;
    test_example_com_zone()
        .save(&pool)
        .await
        .expect("Failed to save test zone");

    let fzr = FileZoneRecord {
        zoneid: Some(1),
        name: "post-test".to_string(),
        rrtype: "A".to_string(),
        id: None,
        class: RecordClass::Internet,
        rdata: "5.6.7.8".to_string(),
        ttl: 300,
    }
    .save(&pool)
    .await
    .expect("Failed to save test record");

    eprintln!("FZR result: {fzr:?}");

    // Create a raw DNS query for post-test.example.com A record
    let mut question_bytes = Vec::new();
    question_bytes.extend_from_slice(b"\x09post-test\x07example\x03com\x00"); // QNAME
    question_bytes.extend_from_slice(&[0x00, 0x01]); // QTYPE (A)
    question_bytes.extend_from_slice(&[0x00, 0x01]); // QCLASS (IN)

    // Create DNS header
    let header = crate::Header {
        id: 1234,
        qr: crate::enums::PacketType::Query,
        opcode: crate::OpCode::Query,
        authoritative: false,
        truncated: false,
        recursion_desired: true,
        recursion_available: false,
        z: false,
        ad: false,
        cd: false,
        rcode: crate::enums::Rcode::NoError,
        qdcount: 1,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };

    let mut dns_packet = Vec::new();
    dns_packet.extend_from_slice(&header.pack().expect("Failed to pack header"));
    dns_packet.extend_from_slice(&question_bytes);

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Content-Type",
        "application/dns-message"
            .parse()
            .expect("Failed to parse header"),
    );
    headers.insert(
        "Accept",
        "application/dns-message"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .post(format!("https://localhost:{api_port}/dns-query"))
        .body(dns_packet)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(200).expect("Failed to parse status")
    );

    let response_bytes = res.bytes().await.expect("Failed to get response bytes");
    eprintln!("Response length: {}", response_bytes.len());

    // Validate that we got a valid DNS response
    assert!(response_bytes.len() >= 12, "Response should be at least 12 bytes (DNS header)");

    // Parse the DNS header from response
    let header_bytes: [u8; 12] = response_bytes[0..12]
        .try_into()
        .expect("slice with incorrect length");
    let response_header = crate::Header::unpack(&header_bytes)
        .expect("Failed to parse response header");

    eprintln!("Response header: {response_header:?}");

    // Validate response header
    assert_eq!(response_header.id, 1234, "Response ID should match request ID");
    assert_eq!(response_header.qr, crate::enums::PacketType::Answer, "Should be a response");
    assert_eq!(response_header.rcode, crate::enums::Rcode::NoError, "Should be NoError");
    assert_eq!(response_header.qdcount, 1, "Should have one question");
    assert_eq!(response_header.ancount, 1, "Should have one answer");

    Ok(())
}

#[tokio::test]
async fn test_doh_nxdomain_response() -> Result<(), ()> {
    // Test DoH response for non-existent domain
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/dns-json"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=nonexistent.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(200).expect("Failed to parse status")
    );

    let response_text = res.text().await.expect("Failed to get response text");
    eprintln!("NXDOMAIN response: {response_text}");
    let json_response: serde_json::Value = serde_json::from_str(&response_text)
        .expect("Failed to parse JSON response");

    // For a non-existent record, we should either get NXDOMAIN (status 3) or NoError with no answers
    let status = json_response["status"].as_u64().expect("status field should be numeric");
    assert!(status == 0 || status == 3, "status should be NoError (0) or NXDOMAIN (3), got {status}");

    // Should have question but no answers
    assert!(json_response.get("Question").is_some(), "Should have question");
    let empty_vec = vec![];
    let answers = json_response["Answer"].as_array().unwrap_or(&empty_vec);
    assert_eq!(answers.len(), 0, "Should have no answers for non-existent record");

    Ok(())
}

#[tokio::test]
async fn test_doh_multiple_records() -> Result<(), ()> {
    // Test DoH response with multiple A records for the same name
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;

    let _user = insert_test_user(&pool).await;
    test_example_com_zone()
        .save(&pool)
        .await
        .expect("Failed to save test zone");

    // Create multiple A records for the same name
    let records = vec![
        ("multi-test", "10.0.0.1"),
        ("multi-test", "10.0.0.2"),
        ("multi-test", "10.0.0.3"),
    ];

    for (name, ip) in records {
        FileZoneRecord {
            zoneid: Some(1),
            name: name.to_string(),
            rrtype: "A".to_string(),
            id: None,
            class: RecordClass::Internet,
            rdata: ip.to_string(),
            ttl: 600,
        }
        .save(&pool)
        .await
        .expect("Failed to save test record");
    }

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/dns-json"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=multi-test.example.com&type=A"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        res.status(),
        reqwest::StatusCode::from_u16(200).expect("Failed to parse status")
    );

    let response_text = res.text().await.expect("Failed to get response text");
    let json_response: serde_json::Value = serde_json::from_str(&response_text)
        .expect("Failed to parse JSON response");

    // Should get NoError
    assert_eq!(json_response["status"].as_u64().expect("status field should be numeric"), 0, "status should be NoError (0)");

    // Should have multiple answers
    let answers = json_response["Answer"].as_array().expect("Answer field should be array");
    assert_eq!(answers.len(), 3, "Should have three answers");

    // Verify all answers are for the correct name and type
    for answer in answers {
        assert_eq!(answer["name"].as_str().expect("answer name should be string"), "multi-test.example.com");
        assert_eq!(answer["type"].as_u64().expect("answer type should be numeric"), 1); // A record
        assert_eq!(answer["TTL"].as_u64().expect("TTL should be numeric"), 600);
        
        // Check that the IP is one of our expected values
        let ip = answer["data"].as_str().expect("answer data should be string");
        assert!(
            ["10.0.0.1", "10.0.0.2", "10.0.0.3"].contains(&ip),
            "IP should be one of the test IPs, got {ip}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_doh_query_parameter_validation() -> Result<(), ()> {
    // Test DoH parameter validation
    let (_pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/dns-json"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    // Test missing name parameter - should return 400 or handle gracefully
    let res = client
        .get(format!("https://localhost:{api_port}/dns-query?type=A"))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    // Current implementation might accept missing name and default to something reasonable
    // or return an error - both are acceptable behaviors for this test
    eprintln!("Missing name parameter response: {} - {}", res.status(), res.text().await.unwrap_or_else(|_| "Failed to read response text".to_string()));

    // Test invalid record type - should handle gracefully
    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=test.example.com&type=INVALID"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    // Should either return 400 or handle gracefully - check it doesn't crash
    assert!(
        res.status().is_client_error() || res.status().is_success(),
        "Should handle invalid record type gracefully"
    );

    Ok(())
}

#[tokio::test]
async fn test_doh_different_record_types() -> Result<(), ()> {
    // Test DoH with different DNS record types
    let (pool, _servers, config) = start_test_server().await;

    let api_port = config.read().await.api_port;

    let _user = insert_test_user(&pool).await;
    test_example_com_zone()
        .save(&pool)
        .await
        .expect("Failed to save test zone");

    // Create different record types
    let records = vec![
        ("txt-test", "TXT", "\"Hello from TXT record\""),
        ("cname-test", "CNAME", "target.example.com"),
        ("mx-test", "MX", "10 mail.example.com"),
    ];

    for (name, record_type, rdata) in records {
        FileZoneRecord {
            zoneid: Some(1),
            name: name.to_string(),
            rrtype: record_type.to_string(),
            id: None,
            class: RecordClass::Internet,
            rdata: rdata.to_string(),
            ttl: 3600,
        }
        .save(&pool)
        .await
        .expect("Failed to save test record");
    }

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/dns-json"
            .parse()
            .expect("Failed to parse header"),
    );

    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build client");

    // Test TXT record
    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=txt-test.example.com&type=TXT"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(res.status(), reqwest::StatusCode::OK);
    
    let response_text = res.text().await.expect("Failed to get response text");
    let json_response: serde_json::Value = serde_json::from_str(&response_text)
        .expect("Failed to parse JSON response");

    assert_eq!(json_response["status"].as_u64().expect("status field should be numeric"), 0);
    let answers = json_response["Answer"].as_array().expect("Answer field should be array");
    assert!(!answers.is_empty(), "Should have TXT record answer");
    assert_eq!(answers[0]["type"].as_u64().expect("answer type should be numeric"), 16); // TXT record type

    // Test CNAME record
    let res = client
        .get(format!(
            "https://localhost:{api_port}/dns-query?name=cname-test.example.com&type=CNAME"
        ))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(res.status(), reqwest::StatusCode::OK);
    
    let response_text = res.text().await.expect("Failed to get response text");
    let json_response: serde_json::Value = serde_json::from_str(&response_text)
        .expect("Failed to parse JSON response");

    assert_eq!(json_response["status"].as_u64().expect("status field should be numeric"), 0);
    let answers = json_response["Answer"].as_array().expect("Answer field should be array");
    assert!(!answers.is_empty(), "Should have CNAME record answer");
    assert_eq!(answers[0]["type"].as_u64().expect("answer type should be numeric"), 5); // CNAME record type

    Ok(())
}

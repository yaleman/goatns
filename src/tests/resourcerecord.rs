use std::fs::read_to_string;
use std::net::SocketAddr;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde_json::Value;
use trust_dns_resolver::config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts};
use trust_dns_resolver::proto::rr::RecordType;
use trust_dns_resolver::AsyncResolver;

use crate::db::DBEntity;
use crate::resourcerecord::check_long_labels;
use crate::tests::test_api::*;
use crate::zones::{FileZone, FileZoneRecord};
use crate::RecordClass;

#[test]
fn test_check_long_labels() {
    assert_eq!(false, check_long_labels(&"hello.".to_string()));
    assert_eq!(false, check_long_labels(&"hello.world".to_string()));
    assert_eq!(
        true,
        check_long_labels(
            &"foo.12345678901234567890123456789012345678901234567890123456789012345678901234567890"
                .to_string()
        )
    );
}

const EXAMPLE_ZONE_NAME: &'static str = "example.com";

async fn test_e2e_record(record: &mut FileZoneRecord) {
    let (pool, _servers, config) = start_test_server().await;

    // create a zone
    let zone = FileZone {
        id: None,
        name: EXAMPLE_ZONE_NAME.to_string(),
        rname: format!("foo.{EXAMPLE_ZONE_NAME}"),
        serial: 12345,
        refresh: 60,
        retry: 60,
        expire: DateTime::<Utc>::MAX_UTC.timestamp() as u32,
        minimum: 60,
        records: Vec::new(),
    }
    .save(&pool)
    .await
    .unwrap();

    // create the record
    record.zoneid = zone.id;

    record
        .save(&pool)
        .await
        .expect("Failed to save zone record");

    // query the record with trustdns
    let mut resolver_config = ResolverConfig::new();
    resolver_config.add_name_server(NameServerConfig::new(
        SocketAddr::new(config.read().address.parse().unwrap(), config.read().port),
        Protocol::Udp,
    ));
    let resolver = AsyncResolver::tokio(resolver_config, ResolverOpts::default());

    let expected_name = format!(
        "{}.",
        vec![&record.name, EXAMPLE_ZONE_NAME]
            .into_iter()
            .filter(|z| !z.is_empty())
            .collect::<Vec<_>>()
            .join(".")
    );
    let query_record = RecordType::from_str(&record.rrtype).expect("Failed to parse");
    eprintln!(
        "Looking up {:?} - parsed RRTYPE: {:?} expected_name: {:?}",
        record, &query_record, &expected_name
    );

    let res = resolver
        .lookup(&expected_name, query_record)
        .await
        .expect("Failed to look it up");
    let rec = res.records().first().unwrap();
    assert_eq!(rec.name().to_string(), expected_name);
}

#[tokio::test]
async fn test_a_record() {
    let mut record = FileZoneRecord {
        zoneid: None,
        id: None,
        name: "test".to_string(),
        rrtype: "A".to_string(),
        class: RecordClass::Internet,
        rdata: "1.2.3.4".to_string(),
        ttl: 60,
    };

    test_e2e_record(&mut record).await;
}

#[tokio::test]
async fn test_aaaa_record() {
    let mut record = FileZoneRecord {
        zoneid: None,
        id: None,
        name: "test".to_string(),
        rrtype: "AAAA".to_string(),
        class: RecordClass::Internet,
        rdata: "f00d::b33f".to_string(),
        ttl: 60,
    };

    test_e2e_record(&mut record).await;
}

#[tokio::test]
async fn test_all_hello() {
    // load the file

    let filepath = format!("{}/hello.goat2.json", env!("CARGO_MANIFEST_DIR"));
    let zone: Value = serde_json::from_str(&read_to_string(&filepath).unwrap()).unwrap();
    let records = zone
        .as_object()
        .unwrap()
        .get("records")
        .unwrap()
        .as_array()
        .unwrap();
    for record in records {
        let mut record: FileZoneRecord = serde_json::from_value(record.clone()).unwrap();
        println!("#########");
        println!("Doing record {:?}", &record);
        println!("#########");
        test_e2e_record(&mut record).await;
    }
    // println!("{:?}", records);
}

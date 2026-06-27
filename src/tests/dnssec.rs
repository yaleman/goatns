use super::prelude::*;

use crate::enums::RecordType;
use crate::resourcerecord::InternalResourceRecord;
use crate::zones::ZoneRecord;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

async fn setup_signed_zone() -> DatabaseConnection {
    let dbconn = test_get_sqlite_memory().await;

    let zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("example.com".to_string()),
        rname: Set("admin.example.com".to_string()),
        serial: Set(2024010101),
        refresh: Set(3600),
        retry: Set(600),
        expire: Set(604800),
        minimum: Set(86400),
        signed: Set(true),
    };
    let zone_model = zone.insert(&dbconn).await.expect("insert zone");

    let _a_record = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone_model.id),
        name: Set(String::new()),
        ttl: Set(Some(3600u32)),
        rrtype: Set(RecordType::A as u16),
        rclass: Set(RecordClass::Internet as u16),
        rdata: Set("192.0.2.1".to_string()),
    }
    .insert(&dbconn)
    .await
    .expect("insert A record");

    let dnskey_rdata = "01010308deadbeef".to_string();
    let __dnskey = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone_model.id),
        name: Set(String::new()),
        ttl: Set(Some(3600u32)),
        rrtype: Set(RecordType::DNSKEY as u16),
        rclass: Set(RecordClass::Internet as u16),
        rdata: Set(dnskey_rdata),
    }
    .insert(&dbconn)
    .await
    .expect("insert DNSKEY record");

    let rrsig_rdata = format!(
        "{:04x}080200000e100058f3c90007{:04x}08example.com",
        RecordType::A as u16,
        12345u16
    );
    let _rrsig = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone_model.id),
        name: Set(String::new()),
        ttl: Set(Some(3600u32)),
        rrtype: Set(RecordType::RRSIG as u16),
        rclass: Set(RecordClass::Internet as u16),
        rdata: Set(rrsig_rdata),
    }
    .insert(&dbconn)
    .await
    .expect("insert RRSIG record");

    dbconn
}

#[tokio::test]
async fn dnssec_signed_zone_returns_ad_bit_and_rrsig() {
    test_logging().await;
    let dbconn = setup_signed_zone().await;

    let qname = b"example.com".to_vec();
    let db_name = String::from_utf8(qname.clone()).expect("invalid UTF-8");
    let records = entities::records_merged::Entity::get_records(
        &dbconn,
        &db_name,
        RecordType::A,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get A records");

    assert!(
        !records.is_empty(),
        "expected at least one A record for example.com"
    );
    let has_a_record = records.iter().any(|r| r.rrtype == RecordType::A as u16);
    assert!(has_a_record, "expected A record in results");

    let has_dnskey = entities::records_merged::Entity::get_records(
        &dbconn,
        &db_name,
        RecordType::DNSKEY,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get DNSKEY records")
    .iter()
    .any(|r| r.rrtype == RecordType::DNSKEY as u16);
    assert!(has_dnskey, "expected DNSKEY record in results");

    let has_rrsig = entities::records_merged::Entity::get_records(
        &dbconn,
        &db_name,
        RecordType::RRSIG,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get RRSIG records")
    .iter()
    .any(|r| r.rrtype == RecordType::RRSIG as u16);
    assert!(has_rrsig, "expected RRSIG record in results");
}

#[tokio::test]
async fn dnssec_signed_zone_ad_bit_in_wire_response() {
    test_logging().await;

    use crate::reply::Reply;
    use crate::resourcerecord::InternalResourceRecord;
    use crate::{Header, PacketType, Question};
    use packed_struct::prelude::*;

    let dbconn = setup_signed_zone().await;

    let mut zr = ZoneRecord {
        name: b"example.com".to_vec(),
        typerecords: vec![],
        signed: true,
    };

    let records = entities::records_merged::Entity::get_records(
        &dbconn,
        "example.com",
        RecordType::A,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get A records");

    zr.typerecords = records
        .into_iter()
        .filter_map(|r| InternalResourceRecord::try_from(r).ok())
        .collect();

    assert!(
        !zr.typerecords.is_empty(),
        "expected A record in ZoneRecord"
    );

    let question = Question {
        qname: b"example.com".to_vec(),
        qtype: RecordType::A,
        qclass: RecordClass::Internet,
    };

    let _ttl = 3600u32;
    let reply = Reply {
        header: Header {
            id: 1234,
            qr: PacketType::Answer,
            opcode: crate::OpCode::Query,
            authoritative: true,
            truncated: false,
            recursion_desired: false,
            recursion_available: false,
            z: false,
            ad: zr.signed,
            cd: false,
            rcode: crate::Rcode::NoError,
            qdcount: 1,
            ancount: zr.typerecords.len() as u16,
            nscount: 0,
            arcount: 0,
        },
        question: Some(question),
        answers: zr.typerecords,
        authorities: vec![],
        additional: vec![],
    };

    let reply_bytes = reply.as_bytes().await.expect("serialize reply");

    assert!(reply_bytes.len() > 12, "reply should have content");

    let header = Header::unpack_from_slice(&reply_bytes[..12]).expect("unpack header");
    assert!(header.ad, "AD bit should be set for signed zone");
    assert_eq!(header.ancount, 1, "ancount should be 1 (A record)");

    drop(dbconn);
}

#[tokio::test]
async fn dnssec_unsigned_zone_no_ad_bit() {
    test_logging().await;

    let dbconn = test_get_sqlite_memory().await;

    let zone = entities::zones::ActiveModel {
        id: NotSet,
        name: Set("unsigned.example".to_string()),
        rname: Set("admin.unsigned.example".to_string()),
        serial: Set(1u32),
        refresh: Set(3600),
        retry: Set(600),
        expire: Set(604800),
        minimum: Set(86400),
        signed: Set(false),
    };
    let zone_model = zone.insert(&dbconn).await.expect("insert zone");

    let _a_record = entities::records::ActiveModel {
        id: NotSet,
        zoneid: Set(zone_model.id),
        name: Set(String::new()),
        ttl: Set(Some(3600u32)),
        rrtype: Set(RecordType::A as u16),
        rclass: Set(RecordClass::Internet as u16),
        rdata: Set("192.0.2.2".to_string()),
    }
    .insert(&dbconn)
    .await
    .expect("insert A record");

    let zone_check = entities::zones::Entity::find()
        .filter(entities::zones::Column::Id.eq(zone_model.id))
        .one(&dbconn)
        .await
        .expect("find zone")
        .expect("zone exists");

    assert!(!zone_check.signed, "zone should be unsigned");

    let records = entities::records_merged::Entity::get_records(
        &dbconn,
        "unsigned.example",
        RecordType::A,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get A records");

    assert!(!records.is_empty(), "should have A record");

    let has_dnskey = entities::records_merged::Entity::get_records(
        &dbconn,
        "unsigned.example",
        RecordType::DNSKEY,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get DNSKEY records");
    assert!(
        has_dnskey.is_empty(),
        "unsigned zone should have no DNSKEY record"
    );

    let zr = ZoneRecord {
        name: b"unsigned.example".to_vec(),
        typerecords: records
            .into_iter()
            .filter_map(|r| InternalResourceRecord::try_from(r).ok())
            .collect(),
        signed: false,
    };

    assert!(!zr.signed);

    drop(dbconn);
}

#[tokio::test]
async fn dnssec_do_bit_triggers_rrsig_lookup() {
    test_logging().await;
    let dbconn = setup_signed_zone().await;

    let records_with_do = entities::records_merged::Entity::get_records(
        &dbconn,
        "example.com",
        RecordType::RRSIG,
        RecordClass::Internet,
        true,
    )
    .await
    .expect("get RRSIG records");

    let rrsig_count = records_with_do
        .iter()
        .filter(|r| r.rrtype == RecordType::RRSIG as u16)
        .count();
    assert!(rrsig_count > 0, "should find RRSIG records for signed zone");

    drop(dbconn);
}

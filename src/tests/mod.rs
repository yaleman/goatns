mod config;
mod db;
mod doh;
mod e2e_test;
mod enums;
mod resourcerecord;
mod test_api;
pub mod test_harness;
mod utils;

use crate::db::test::test_get_sqlite_memory;
use crate::db::*;
use crate::enums::{RecordClass, RecordType};
use crate::resourcerecord::{InternalResourceRecord, LocRecord};
use crate::tests::test_harness::*;
use crate::utils::name_as_bytes;
use crate::{get_question_qname, PacketType, Question};
use ipnet::IpNet;
use log::debug;
use packed_struct::prelude::*;
use std::net::IpAddr;
use std::str::FromStr;

#[test]
/// test my assumptions about ipnet things
fn test_ip_in_ipnet() {
    let net = IpNet::from_str("10.0.0.0/24").unwrap();

    let addr: IpAddr = "10.0.0.69".parse().unwrap();
    let noaddr: IpAddr = "69.0.0.69".parse().unwrap();

    assert!(net.contains(&addr));
    assert!(!net.contains(&noaddr));
}

#[test]
fn test_resourcerecord_name_to_bytes() {
    let rdata: Vec<u8> = "cheese.world".as_bytes().to_vec();
    assert_eq!(
        name_as_bytes(rdata, None, None),
        [6, 99, 104, 101, 101, 115, 101, 5, 119, 111, 114, 108, 100, 0]
    );
}
#[test]
fn test_resourcerecord_short_name_to_bytes() {
    let rdata = "cheese".as_bytes().to_vec();
    assert_eq!(
        name_as_bytes(rdata, None, None),
        [6, 99, 104, 101, 101, 115, 101, 0]
    );
}
#[test]
fn test_name_as_bytes() {
    let rdata = "cheese.hello.world".as_bytes().to_vec();
    let compress_ref = "zing.hello.world".as_bytes().to_vec();
    assert_eq!(
        name_as_bytes(rdata, Some(12u16), Some(&compress_ref)),
        [6, 99, 104, 101, 101, 115, 101, 192, 17]
    );
}

#[tokio::test]
async fn test_build_iana_org_a_reply() {
    use crate::reply::Reply;
    use crate::resourcerecord::InternalResourceRecord;
    use crate::Header;

    let header = Header {
        id: 41840,
        qr: PacketType::Answer,
        opcode: crate::OpCode::Query,
        authoritative: false,
        truncated: false,
        recursion_desired: true,
        recursion_available: true,
        z: false,
        ad: false,
        cd: false,
        rcode: crate::Rcode::NoError,
        qdcount: 1,
        ancount: 1,
        arcount: 0,
        nscount: 0,
    };
    let qname = "iana.org".as_bytes().to_vec();
    let question = Question {
        qname: qname.clone(),
        qtype: crate::RecordType::A,
        qclass: crate::RecordClass::Internet,
    };
    let question_length = question.to_bytes().len();
    debug!("question byte length: {}", question_length);
    let address = std::net::Ipv4Addr::from_str("192.0.43.8").unwrap();
    let answers = vec![InternalResourceRecord::A {
        ttl: 350,
        address: address.into(),
        rclass: crate::RecordClass::Internet,
    }];
    let reply = Reply {
        header,
        question: Some(question),
        answers,
        authorities: vec![],
        additional: vec![],
    };
    let reply_bytes: Vec<u8> = reply.as_bytes().await.unwrap();
    debug!("{:?}", reply_bytes);
    let expected_bytes = [
        /* header - 12 bytes */
        0xa3, 0x70, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        /* question - 14 bytes */
        0x04, 0x69, 0x61, 0x6e, 0x61, 0x03, 0x6f, 0x72, 0x67, 0x00, 0x00, 0x01, 0x00, 0x01,
        /* answer - 16 bytes */
        0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x5e, 0x00, 0x04, 0xc0, 0x00, 0x2b,
        0x08,
    ];
    let mut current_block: &str;
    for (index, byte) in reply_bytes.iter().enumerate() {
        if index < 12 {
            current_block = "Header ";
        } else if index < 26 {
            current_block = "Question ";
        } else {
            current_block = "Answer   ";
        }
        match expected_bytes.get(index) {
            Some(expected_byte) => debug!(
                "{} \t {} us: {} ex: {} {}",
                current_block,
                index,
                byte,
                expected_byte,
                (byte == expected_byte)
            ),
            None => {
                panic!("Our reply is longer!");
                // break;
            }
        }
        assert_eq!(byte, &expected_bytes[index]);
    }
    assert_eq!([reply_bytes[0], reply_bytes[1]], [0xA3, 0x70])
}
#[tokio::test]
async fn test_cloudflare_soa_reply() {
    use crate::reply::Reply;
    use crate::resourcerecord::DomainName;
    use crate::{Header, HEADER_BYTES};
    //     /*
    //     from: <https://raw.githubusercontent.com/paulc/dnslib/master/dnslib/test/cloudflare.com-SOA>

    //     ;; Sending:
    //     ;; QUERY: 8928010000010000000000000a636c6f7564666c61726503636f6d0000060001
    //     ;; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 35112
    //     ;; flags: rd; QUERY: 1, ANSWER: 0, AUTHORITY: 0, ADDITIONAL: 0
    //     ;; QUESTION SECTION:
    //     ;cloudflare.com.                IN      SOA

    //     ;; Got answer:
    //     ;; RESPONSE: 8928818000010001000000000a636c6f7564666c61726503636f6d0000060001c00c00060001000000ad0020036e7333c00c03646e73c00c7906ce18000027100000096000093a800000012c
    //     ;; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 35112
    //     ;; flags: qr rd ra; QUERY: 1, ANSWER: 1, AUTHORITY: 0, ADDITIONAL: 0
    //     ;; QUESTION SECTION:
    //     ;cloudflare.com.                IN      SOA
    //     ;; ANSWER SECTION:
    //     cloudflare.com.         173     IN      SOA     ns3.cloudflare.com. dns.cloudflare.com. 2030489112 10000 2400 604800 300

    //     */
    let original_question: [u8; 32] = [
        0x89, 0x28, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x63, 0x6c,
        0x6f, 0x75, 0x64, 0x66, 0x6c, 0x61, 0x72, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x06,
        0x00, 0x01,
    ];

    let expected_bytes = [
        /* header - 12 bytes */
        0x89, 0x28, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        /* question - 14 bytes */
        0x0a, 0x63, 0x6c, 0x6f, 0x75, 0x64, 0x66, 0x6c, 0x61, /* answer - 16 bytes */
        0x72, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x06, 0x00, 0x01, 0xc0, 0x0c, 0x00, 0x06,
        0x00, 0x01, 0x00, 0x00, 0x00, 0xad, 0x00, 0x20, 0x03, 0x6e, 0x73, 0x33, 0xc0, 0x0c, 0x03,
        0x64, 0x6e, 0x73, 0xc0, 0x0c, 0x79, 0x06, 0xce, 0x18, 0x00, 0x00, 0x27, 0x10, 0x00, 0x00,
        0x09, 0x60, 0x00, 0x09, 0x3a, 0x80, 0x00, 0x00, 0x01, 0x2c,
    ];

    let header = Header {
        id: 35112,
        qr: PacketType::Answer,
        opcode: crate::OpCode::Query,
        authoritative: false,
        truncated: false,
        recursion_desired: true,
        recursion_available: false,
        z: false,
        ad: false,
        cd: false,
        rcode: crate::Rcode::NoError,
        qdcount: 1,
        ancount: 1,
        arcount: 0,
        nscount: 0,
    };
    let qname = "cloudflare.com".as_bytes().to_vec();
    let question = Question {
        qname: qname.clone(),
        qtype: crate::RecordType::SOA,
        qclass: crate::RecordClass::Internet,
    };
    let question_length = question.to_bytes().len();
    debug!("question byte length: {}", question_length);

    // YOLO the  string conversions because it's a test
    let rdata = InternalResourceRecord::SOA {
        zone: DomainName::from("cloudflare.com"),
        mname: DomainName::from("ns3.cloudflare.com"),
        rname: DomainName::from("dns.cloudflare.com"),
        serial: 2030489112,
        refresh: 10000,
        retry: 2400,
        expire: 604800,
        minimum: 300,
        rclass: crate::RecordClass::Internet,
    };

    // let rdata = rdata.as_bytes();
    let answers = vec![rdata];

    let mut reply = Reply {
        header: header.clone(),
        question: Some(question),
        answers,
        authorities: vec![],
        additional: vec![],
    };
    reply.header.recursion_available = true;
    debug!("{:?}", reply);
    let reply_bytes: Vec<u8> = reply.as_bytes().await.unwrap();
    debug!("{:?}", reply_bytes);

    // testing if I was parsing it right...
    let mut their_header = Header::unpack_from_slice(&original_question[0..HEADER_BYTES]).unwrap();
    their_header.ancount = 1;
    assert_eq!(header, their_header.as_answer());
    log::trace!("Parsed header matched!");

    let mut current_block: &str;
    for (index, byte) in reply_bytes.iter().enumerate() {
        if index < HEADER_BYTES {
            current_block = "Header ";
        } else if index < HEADER_BYTES + 9 {
            current_block = "Question ";
        } else {
            current_block = "Answer   ";
        }

        let b = [byte.clone()];
        let ascii_byte_us = match byte.is_ascii_alphanumeric() {
            true => std::str::from_utf8(&b).unwrap_or("-"),
            false => " ",
        };

        match expected_bytes.get(index) {
            Some(expected_byte) => {
                let eb: u8 = expected_byte.clone();
                let ascii_byte_them = match eb.is_ascii_alphanumeric() {
                    true => std::str::from_utf8(&b).unwrap_or("-"),
                    false => " ",
                };
                log::trace!(
                    "{current_block} \t {index} us: {}\t{:#010b}\tex: {expected_byte}\t{expected_byte:#010b} \tchars: {} {}\t matched: {}",
                    byte.clone(),
                    byte.clone(),
                    ascii_byte_us,
                    ascii_byte_them,
                    (byte == expected_byte)
                )
            }
            None => {
                panic!("Our reply is longer!");
                // break;
            }
        }
        // assert_eq!(byte, &expected_bytes[index]);
    }
    // assert_eq!([reply_bytes[0], reply_bytes[1]], [0xA3, 0x70])
}

#[tokio::test]
async fn build_ackcdn_allzeros() {
    use crate::reply::Reply;
    use crate::Header;

    let header = Header {
        id: 0x3DE1,
        qr: PacketType::Answer,
        opcode: crate::OpCode::Query,
        authoritative: true,
        truncated: false,
        recursion_desired: true,
        recursion_available: true,
        z: false,
        ad: false,
        cd: false,
        rcode: crate::Rcode::NoError,
        qdcount: 1,
        ancount: 1,
        arcount: 0,
        nscount: 0,
    };
    let qname = "ackcdn.com".as_bytes().to_vec();
    let question = Question {
        qname: qname.clone(),
        qtype: crate::RecordType::A,
        qclass: crate::RecordClass::Internet,
    };
    let question_length = question.to_bytes().len();
    debug!("question byte length: {}", question_length);

    // let rdata = IpAddr::try_from("0.0.0.0");
    // let rdata: Ipv4Addr = "0.0.0.0".parse().unwrap();
    // let rdata: u32 = rdata.into();
    // let rdata = rdata.octets();
    // let rdlength: u16 = rdata.len() as u16;

    let answers = vec![crate::resourcerecord::InternalResourceRecord::A {
        // name: vec![0xc0, 0x0c],
        // name: "ackcdn.com".as_bytes().to_vec(),
        // record_type: crate::RecordType::A,
        // class: crate::RecordClass::Internet,
        ttl: 2u32,
        address: 0u32,

        rclass: crate::RecordClass::Internet,
        // rdlength,
        // rdata: rdata.into(),
        // compression: true,
    }];

    let reply = Reply {
        header,
        question: Some(question),
        answers,
        authorities: vec![],
        additional: vec![],
    };
    let reply_bytes: Vec<u8> = reply.as_bytes().await.unwrap();
    debug!("{} bytes: {:?}", reply_bytes.len(), reply_bytes);

    let expected_bytes = [
        /* header - 12 bytes */
        0x3d, 0xe1, 0x85, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        /* question - 16 bytes */
        0x06, 0x61, 0x63, 0x6b, 0x63, 0x64, 0x6e, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01, 0x00,
        0x01, /* answer - 16 bytes  */
        0xC0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00,
        0x00,
    ];

    log::trace!("Our length: {}", reply_bytes.len());
    log::trace!("Exp length: {}", expected_bytes.len());

    let mut current_block: &str;
    for (index, byte) in reply_bytes.iter().enumerate() {
        if index < 12 {
            current_block = "Header ";
        } else if index < 28 {
            current_block = "Question ";
        } else {
            current_block = "Answer   ";
        }
        match expected_bytes.get(index) {
            Some(expected_byte) => log::error!(
                "{} \t {} us: {} ex: {} {}",
                current_block,
                index,
                byte,
                expected_byte,
                (byte == expected_byte)
            ),
            None => {
                panic!("Our reply is longer!");
                // break;
            }
        }
        assert_eq!(byte, &expected_bytes[index]);
    }
    assert_eq!([reply_bytes[0], reply_bytes[1]], [0x3D, 0xE1]);
}

/// turns a degrees-minutes-seconds input into a signed 32-bit integer.
/// when positive = true, you're North or West
#[test]
fn test_dms_to_i32() {
    use crate::utils::dms_to_u32;

    let equator: u32 = 2u32.pow(31);
    assert_eq!(dms_to_u32(0, 0, 0.0, true), equator);

    assert_eq!(dms_to_u32(1, 2, 3.0, true), 2151206648);
}

#[test]
/// this is based on a record in a pcap in the examples directory -
fn test_loc_as_bytes() {
    // pizza.yaleman.org.	69	IN	LOC	1 2 3.000 N 1 2 3.000 E 10.00m 10m 10m 10m
    // compression header c00c
    // LOC: 001d
    // IN: 0001
    // TTL: 00000045 (69)
    // Length: 0010 (16)
    let expected_bytes: [u8; 16] = [
        0x00, // Version: 0
        0x13, // size 19 (10m)
        0x13, // hor pres 19 (10m)
        0x13, // ver pres 19 (10m)
        0x80, 0x38, 0xce, 0xf8, // Latitude: 2151206648 (1 deg 2 min 3.000 sec N)
        0x80, 0x38, 0xce, 0xf8, // Longitude: 2151206648 (1 deg 2 min 3.000 sec E)
        0x00, 0x98, 0x9a, 0x68, // Altitude: 10001000 (10 m)
    ];

    let test_record = LocRecord {
        version: 0,
        size: 0x13,
        horiz_pre: 0x13,
        vert_pre: 0x13,
        latitude: 2151206648,
        longitude: 2151206648,
        altitude: 10001000,
    };
    let test_result = test_record.pack().unwrap();
    assert_eq!(expected_bytes, test_result);
}

#[test]
fn test_loc_record_parser() {
    use crate::resourcerecord::FileLocRecord;

    let sample_data: Vec<(&str, FileLocRecord)> = vec![
        (
            "42 21 43.952 N 71 5 6.344 W -24m 1m 200m 15m",
            FileLocRecord {
                d1: 42,
                m1: 21,
                s1: 43.952,
                d2: 71,
                m2: 5,
                s2: 6.344,
                lat_dir: "N".to_string(),
                lon_dir: "W".to_string(),
                alt: (10000000 + (-24 * 100)),
                size: 0x12,
                horiz_pre: 0x24,
                vert_pre: 0x23,
            },
        ),
        (
            "42 21 43.952 N 71 5 6.344 W -24m 1m 200m",
            FileLocRecord {
                d1: 42,
                m1: 21,
                s1: 43.952,
                lat_dir: "N".to_string(),
                d2: 71,
                m2: 5,
                s2: 6.344,
                lon_dir: "W".to_string(),
                alt: 10000000 + (-24 * 100),
                size: 0x12,
                horiz_pre: 0x24,
                vert_pre: 0x13,
            },
        ),
        (
            "32 S 116 E 10m",
            FileLocRecord {
                d1: 32,
                lat_dir: "S".to_string(),
                d2: 116,
                lon_dir: "E".to_string(),
                alt: 10000000 + (10 * 100),
                ..Default::default()
            },
        ),
        (
            "32 7 19 S 116 2 25 E 10m",
            FileLocRecord {
                d1: 32,
                m1: 7,
                s1: 19.0,
                lat_dir: "S".to_string(),
                d2: 116,
                m2: 2,
                s2: 25.0,
                lon_dir: "E".to_string(),
                alt: 10000000 + (10 * 100),
                ..FileLocRecord::default()
            },
        ),
        (
            "42 21 54 N 71 06 18 W -24m 30m",
            FileLocRecord {
                d1: 42,
                m1: 21,
                s1: 54.0,
                lat_dir: "N".to_string(),
                d2: 71,
                m2: 6,
                s2: 18.0,
                lon_dir: "W".to_string(),
                alt: (-24 * 100) + 10000000,
                size: 0x33,
                ..FileLocRecord::default()
            },
        ),
        (
            "52 14 05 N 00 08 50 E 10m",
            FileLocRecord {
                d1: 52,
                m1: 14,
                s1: 5.0,
                lat_dir: "N".to_string(),
                m2: 8,
                s2: 50.0,
                lon_dir: "E".to_string(),
                alt: (10 * 100) + 10000000,
                ..FileLocRecord::default()
            },
        ),
        (
            "42 21 28.764 N 71 00 51.617 W -44m 2000m",
            FileLocRecord {
                d1: 42,
                m1: 21,
                s1: 28.764,
                lat_dir: "N".to_string(),
                d2: 71,
                m2: 0,
                s2: 51.617,
                lon_dir: "W".to_string(),
                alt: (-44 * 100) + 10000000,
                size: 0x25,
                ..FileLocRecord::default()
            },
        ),
    ];
    for (input, output) in sample_data {
        eprintln!("Testing {input}");
        let record = FileLocRecord::try_from(input);
        assert!(record.is_ok());
        assert_eq!(record.unwrap(), output);
    }
}

#[test]
fn test_all_record_type_conversions() {
    for record_type in enum_iterator::all::<RecordType>().collect::<Vec<_>>() {
        eprintln!("Testing {record_type:?}");
        if record_type != RecordType::InvalidType {
            let string_version = record_type.to_string();
            assert_ne!(string_version, "".to_string());

            let from_string_version = RecordType::from(string_version);

            assert_eq!(record_type, from_string_version);
            let _: u16 = record_type as u16;
        } else {
            let garbled = String::from("asdfasdfasdflq23423l4kj23h4l23jk4");
            assert_eq!(record_type, RecordType::from(garbled));
            assert_eq!(record_type, RecordType::from(&12345u16));
        }
    }
    // panic!();
}
#[test]
fn test_all_record_class_conversions() {
    for record_class in enum_iterator::all::<RecordClass>().collect::<Vec<_>>() {
        eprintln!("Testing {record_class:?}");
        let fail_string: &'static str = "";
        if record_class != RecordClass::InvalidType {
            let string_version = record_class.to_owned().to_string();
            assert_ne!(string_version, fail_string);

            let str_version = string_version.as_str();

            let from_string_version = RecordClass::from(str_version);

            assert_eq!(record_class, from_string_version);
            let _: u16 = record_class as u16;
        } else {
            let garbled = "asdfasdfasdflq23423l4kj23h4l23jk4";
            assert_eq!(record_class, RecordClass::from(garbled));
            assert_eq!(record_class, RecordClass::from(&12345u16));
        }
    }
    // panic!();
}

#[test]
fn test_normalize_name() {
    let q = Question {
        qname: String::from("HellO.world").into_bytes(),
        qtype: crate::enums::RecordType::A,
        qclass: crate::enums::RecordClass::Internet,
    };
    assert_eq!(q.normalized_name().unwrap(), String::from("hello.world"));
    let q = Question {
        qname: String::from("hello.world").into_bytes(),
        qtype: crate::enums::RecordType::A,
        qclass: crate::enums::RecordClass::Internet,
    };
    assert_eq!(q.normalized_name().unwrap(), String::from("hello.world"));
}

#[test]
fn test_get_question_qname() {
    assert!(get_question_qname(&[23, 0]).is_err());

    let sample_data = vec![7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0];
    eprintln!("{:?}", sample_data);
    let result = get_question_qname(&sample_data);
    assert_eq!(
        result,
        Ok(vec![101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109])
    );
}

#[tokio::test]
///tries to test when input buffers are weird
async fn test_question_from_bytes() {
    let ok_question = vec![
        /* question - 14 bytes */
        0x04, 0x69, 0x61, 0x6e, 0x61, 0x03, 0x6f, 0x72, 0x67, 0x00, 0x00, 0x01, //0x00, 0x01,
    ];
    let input_bufs: Vec<Vec<u8>> = vec![
        /* header - 12 bytes */
        // 0xa3, 0x70, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ok_question[0..10].to_vec(),
        ok_question[0..11].to_vec(),
        ok_question[0..12].to_vec(),
    ];

    for buf in input_bufs {
        if Question::from_packets(&buf).is_ok() {
            panic!("This should bail!");
        }
    }
}

#[tokio::test]
/// this test checks that all TTLs in the record are the same when we set normalise_ttls = true
async fn test_normalize_ttls() {
    // use crate::zones::FileZoneRecord;
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await.unwrap();
    import_test_zone_file(&pool).await.unwrap();

    let response = get_records(
        &pool,
        "ttltest.hello.goat".to_string(),
        RecordType::A,
        RecordClass::Internet,
        true,
    )
    .await
    .unwrap();

    print!("Checking that we got three records...");
    println!("Response:");
    for re in response.iter() {
        println!("{re:?}");
    }
    assert_eq!(response.iter().len(), 3);
    println!(" OK");

    // first we just check we got three records from the db
    let mut found_records: Vec<u32> = vec![];
    for record in response {
        println!("found record {record:?}");
        if let InternalResourceRecord::A { ttl, .. } = record {
            if !found_records.contains(&ttl) {
                found_records.push(ttl);
            }
        } else {
            println!("We found a record that wasn't an A record, that's cool I guess?")
        }
    }
    print!("Checking that we found a single ttl...");
    assert!(found_records.len() == 1);
    println!(" OK");
}

#[tokio::test]
/// this test checks that all TTLs in the record are the same when we set normalise_ttls = true
async fn test_dont_normalize_ttls() {
    // use crate::zones::FileZoneRecord;
    let pool = test_get_sqlite_memory().await;

    start_db(&pool).await.unwrap();
    import_test_zone_file(&pool).await.unwrap();

    let response = get_records(
        &pool,
        "ttltest.hello.goat".to_string(),
        RecordType::A,
        RecordClass::Internet,
        false,
    )
    .await
    .unwrap();

    print!("Checking that we got three records...");
    assert!(response.iter().len() == 3);
    println!(" OK");

    // first we just check we got three records from the db
    let mut found_records: Vec<u32> = vec![];
    for record in response {
        println!("found record {record:?}");
        if let InternalResourceRecord::A { ttl, .. } = record {
            if !found_records.contains(&ttl) {
                found_records.push(ttl);
            }
        } else {
            println!("We found a record that wasn't an A record, that's cool I guess?")
        }
    }
    print!("Checking that we found three different ttls...");
    assert!(found_records.len() == 3);
    println!(" OK");
}

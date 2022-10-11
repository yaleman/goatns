#[cfg(test)]
mod tests {
    use crate::resourcerecord::InternalResourceRecord;
    use crate::utils::name_as_bytes;
    use crate::{PacketType, Question};
    use packed_struct::prelude::*;
    use std::net::Ipv4Addr;
    // , ResourceRecord
    use log::debug;

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

    // #[tokio::test]
    // async fn test_build_iana_org_a_reply() {
    //     use crate::{Header, Reply};
    //     use crate::resourcerecord::InternalResourceRecord;

    //     let header = Header {
    //         id: 41840,
    //         qr: PacketType::Answer,
    //         opcode: crate::OpCode::Query,
    //         authoritative: false,
    //         truncated: false,
    //         recursion_desired: true,
    //         recursion_available: true,
    //         z: false,
    //         ad: false,
    //         cd: false,
    //         rcode: crate::Rcode::NoError,
    //         qdcount: 1,
    //         ancount: 1,
    //         arcount: 0,
    //         nscount: 0,
    //     };
    //     let qname = "iana.org".as_bytes().to_vec();
    //     let question = Question {
    //         qname: qname.clone(),
    //         qtype: crate::RecordType::A,
    //         qclass: crate::RecordClass::Internet,
    //     };
    //     let question_length = question.to_bytes().len();
    //     debug!("question byte length: {}", question_length);
    //     let answers = vec![InternalResourceRecord {
    //         name: qname,
    //         record_type: crate::RecordType::A,
    //         class: crate::RecordClass::Internet,
    //         ttl: 350,
    //         rdlength: 4,
    //         rdata: IPAddress::new(192, 0, 43, 8).pack().unwrap().into(),
    //         compression: true,
    //     }];

    //     let mut reply = Reply {
    //         header,
    //         question: Some(question),
    //         answers,
    //         authorities: vec![],
    //         additional: vec![],
    //     };
    //     let reply_bytes: Vec<u8> = reply.as_bytes().unwrap();
    //     debug!("{:?}", reply_bytes);

    //     let expected_bytes = [
    //         /* header - 12 bytes */
    //         0xa3, 0x70, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    //         /* question - 14 bytes */
    //         0x04, 0x69, 0x61, 0x6e, 0x61, 0x03, 0x6f, 0x72, 0x67, 0x00, 0x00, 0x01, 0x00, 0x01,
    //         /* answer - 16 bytes */
    //         0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x5e, 0x00, 0x04, 0xc0, 0x00,
    //         0x2b, 0x08,
    //     ];

    //     let mut current_block: &str;
    //     for (index, byte) in reply_bytes.iter().enumerate() {
    //         if index < 12 {
    //             current_block = "Header ";
    //         } else if index < 26 {
    //             current_block = "Question ";
    //         } else {
    //             current_block = "Answer   ";
    //         }
    //         match expected_bytes.get(index) {
    //             Some(expected_byte) => debug!(
    //                 "{} \t {} us: {} ex: {} {}",
    //                 current_block,
    //                 index,
    //                 byte,
    //                 expected_byte,
    //                 (byte == expected_byte)
    //             ),
    //             None => {
    //                 panic!("Our reply is longer!");
    //                 // break;
    //             }
    //         }
    //         assert_eq!(byte, &expected_bytes[index]);
    //     }
    //     assert_eq!([reply_bytes[0], reply_bytes[1]], [0xA3, 0x70])
    // }

    #[tokio::test]
    async fn test_cloudflare_soa_reply() {
        use crate::resourcerecord::DomainName;
        use crate::ResourceRecord;
        use crate::{Header, Reply, HEADER_BYTES};
        //     /*
        //     from: https://raw.githubusercontent.com/paulc/dnslib/master/dnslib/test/cloudflare.com-SOA

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
            0x89, 0x28, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x63,
            0x6c, 0x6f, 0x75, 0x64, 0x66, 0x6c, 0x61, 0x72, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00,
            0x00, 0x06, 0x00, 0x01,
        ];

        let expected_bytes = [
            /* header - 12 bytes */
            0x89, 0x28, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            /* question - 14 bytes */
            0x0a, 0x63, 0x6c, 0x6f, 0x75, 0x64, 0x66, 0x6c, 0x61, /* answer - 16 bytes */
            0x72, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x06, 0x00, 0x01, 0xc0, 0x0c, 0x00,
            0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0xad, 0x00, 0x20, 0x03, 0x6e, 0x73, 0x33, 0xc0,
            0x0c, 0x03, 0x64, 0x6e, 0x73, 0xc0, 0x0c, 0x79, 0x06, 0xce, 0x18, 0x00, 0x00, 0x27,
            0x10, 0x00, 0x00, 0x09, 0x60, 0x00, 0x09, 0x3a, 0x80, 0x00, 0x00, 0x01, 0x2c,
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
        };

        let rdata = rdata.as_bytes();
        let answers = vec![ResourceRecord {
            name: qname,
            record_type: crate::RecordType::SOA,
            class: crate::RecordClass::Internet,
            ttl: 173,
            rdata: rdata,
        }];

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
        let mut their_header =
            Header::unpack_from_slice(&original_question[0..HEADER_BYTES]).unwrap();
        their_header.ancount = 1;
        assert_eq!(header, their_header.as_answer());
        eprintln!("Parsed header matched!");

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
                    eprintln!(
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
        assert_eq!([reply_bytes[0], reply_bytes[1]], [0xA3, 0x70])
    }

    #[tokio::test]
    async fn test_build_ackcdn_allzeros() {
        use crate::{Header, Reply};

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
        let rdata: Ipv4Addr = "0.0.0.0".parse().unwrap();
        let rdata = rdata.octets();
        // let rdlength: u16 = rdata.len() as u16;

        let answers = vec![crate::ResourceRecord {
            name: vec![0xc0, 0x0c],
            record_type: crate::RecordType::A,
            class: crate::RecordClass::Internet,
            ttl: 2,
            // rdlength,
            rdata: rdata.into(),
            // compression: true,
        }];

        let mut reply = Reply {
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
            0x06, 0x61, 0x63, 0x6b, 0x63, 0x64, 0x6e, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01,
            0x00, 0x01, /* answer - 16 bytes  */
            0xC0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00,
            0x00, 0x00,
        ];

        debug!("Our length: {}", reply_bytes.len());
        debug!("Exp length: {}", expected_bytes.len());

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
        assert_eq!([reply_bytes[0], reply_bytes[1]], [0x3D, 0xE1]);
    }

    // #[tokio::test]
    // async fn test_from_bytes() {
    //     use crate::UDP_BUFFER_SIZE;
    //     let input = [
    //         0x9c, 0x58, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x79,
    //         0x61, 0x6c, 0x65, 0x6d, 0x61, 0x6e, 0x03, 0x6f, 0x72, 0x67, 0x00, 0x00, 0x01, 0x00,
    //         0x01,
    //     ];

    //     let mut buf: [u8; UDP_BUFFER_SIZE] = [0; UDP_BUFFER_SIZE];
    //     for (i, b) in input.iter().enumerate() {
    //         buf[i] = *b as u8;
    //     }

    //     let result = crate::parse_udp_query(crate::enums::Protocol::Udp, input.len(), buf, false)
    //         .await
    //         .unwrap();

    //     assert_eq!(
    //         result.question.as_ref().unwrap().qclass,
    //         crate::enums::RecordClass::Internet
    //     );
    //     assert_eq!(
    //         result.question.as_ref().unwrap().qname,
    //         "yaleman.org".as_bytes().to_vec()
    //     );
    //     // TODO: make sure *everything* is right here
    // }
}

#[cfg(test)]
mod tests {
    use crate::ip_address::IPAddress;
    use crate::resourcerecord::RdataSOA;
    use crate::utils::name_as_bytes;
    use crate::{PacketType, Question, ResourceRecord, HEADER_BYTES};
    use log::debug;
    use packed_struct::prelude::*;

    #[test]
    fn test_resourcerecord_name_to_bytes() {
        let rdata: Vec<u8> = "cheese.world".as_bytes().to_vec();
        assert_eq!(
            name_as_bytes(rdata, None),
            [6, 99, 104, 101, 101, 115, 101, 5, 119, 111, 114, 108, 100, 0]
        );
    }
    #[test]
    fn test_resourcerecord_short_name_to_bytes() {
        let rdata = "cheese".as_bytes().to_vec();
        assert_eq!(
            name_as_bytes(rdata, None),
            [6, 99, 104, 101, 101, 115, 101, 0]
        );
    }

    #[test]
    fn test_ipaddress() {
        let iptest: IPAddress = IPAddress::new(1, 1, 1, 1);
        let output: [u8; 4] = match iptest.pack() {
            Ok(value) => value,
            Err(error) => {
                panic!("{:?}", error)
            }
        };
        let result: u32 = u32::from_be_bytes(output);
        assert_eq!(result, 16843009);

        let iptest: IPAddress = IPAddress::new(123, 145, 31, 71);
        let output: [u8; 4] = match iptest.pack() {
            Ok(value) => value,
            Err(error) => {
                panic!("{:?}", error)
            }
        };
        let result: u32 = u32::from_be_bytes(output);
        assert_eq!(result, 2073108295);
    }

    #[tokio::test]
    async fn test_build_iana_org_a_reply() {
        use crate::{Header, Reply};

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
        let answers = vec![ResourceRecord {
            name: qname,
            record_type: crate::RecordType::A,
            class: crate::RecordClass::Internet,
            ttl: 350,
            rdlength: 4,
            rdata: IPAddress::new(192, 0, 43, 8).pack().unwrap().into(),
            compression: true,
        }];

        let mut reply = Reply {
            header,
            question: Some(question),
            answers,
            authorities: vec![],
            additional: vec![],
        };
        let reply_bytes: Vec<u8> = reply.as_bytes().unwrap();
        debug!("{:?}", reply_bytes);

        let expected_bytes = [
            /* header - 12 bytes */
            0xa3, 0x70, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            /* question - 14 bytes */
            0x04, 0x69, 0x61, 0x6e, 0x61, 0x03, 0x6f, 0x72, 0x67, 0x00, 0x00, 0x01, 0x00, 0x01,
            /* answer - 16 bytes */
            0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x5e, 0x00, 0x04, 0xc0, 0x00,
            0x2b, 0x08,
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
        use crate::{Header, Reply};
        /*

        ;; Got answer:
        ;; RESPONSE: 8928818000010001000000000a636c6f7564666c61726503636f6d0000060001c00c00060001000000ad0020036e7333c00c03646e73c00c7906ce18000027100000096000093a800000012c
        ;; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 35112
        ;; flags: qr rd ra; QUERY: 1, ANSWER: 1, AUTHORITY: 0, ADDITIONAL: 0
        ;; QUESTION SECTION:
        ;cloudflare.com.                IN      SOA
        ;; ANSWER SECTION:
        cloudflare.com.         173     IN      SOA     ns3.cloudflare.com. dns.cloudflare.com. 2030489112 10000 2400 604800 300
        */
        let header = Header {
            id: 35112,
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
        let qname = "cloudflare.com".as_bytes().to_vec();
        let question = Question {
            qname: qname.clone(),
            qtype: crate::RecordType::SOA,
            qclass: crate::RecordClass::Internet,
        };
        let question_length = question.to_bytes().len();
        debug!("question byte length: {}", question_length);

        let rdata = RdataSOA {
            mname: question.qname.clone(),
            rname: "dns.cloudflare.com".as_bytes().to_vec(),
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
            rdlength: rdata.len() as u16,
            rdata: rdata,
            compression: false,
        }];

        let mut reply = Reply {
            header,
            question: Some(question),
            answers,
            authorities: vec![],
            additional: vec![],
        };
        let reply_bytes: Vec<u8> = reply.as_bytes().unwrap();
        debug!("{:?}", reply_bytes);

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

        let mut current_block: &str;
        for (index, byte) in reply_bytes.iter().enumerate() {
            if index < HEADER_BYTES {
                current_block = "Header ";
            } else if index < HEADER_BYTES + 9 {
                current_block = "Question ";
            } else {
                current_block = "Answer   ";
            }
            match expected_bytes.get(index) {
                Some(expected_byte) => eprintln!(
                    "{} \t {} us: {}\tex: {}\tchar: {}\t matched: {}",
                    current_block,
                    index,
                    byte.clone(),
                    expected_byte,
                    std::str::from_utf8(&[byte.clone()]).unwrap_or("-"),
                    (byte == expected_byte)
                ),
                None => {
                    panic!("Our reply is longer!");
                    // break;
                }
            }
            // assert_eq!(byte, &expected_bytes[index]);
        }
        assert_eq!([reply_bytes[0], reply_bytes[1]], [0xA3, 0x70])
    }

    #[test]
    fn test_build_ackcdn_allzeros() {
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

        let rdata = IPAddress::new(0, 0, 0, 0).pack().unwrap();
        // debug!("rdata: {:?}", rdata);
        let rdlength: u16 = rdata.len() as u16;

        let answers = vec![crate::ResourceRecord {
            name: vec![0xc0, 0x0c],
            record_type: crate::RecordType::A,
            class: crate::RecordClass::Internet,
            ttl: 2,
            rdlength,
            rdata: rdata.into(),
            compression: true,
        }];

        let mut reply = Reply {
            header,
            question: Some(question),
            answers,
            authorities: vec![],
            additional: vec![],
        };
        let reply_bytes: Vec<u8> = reply.as_bytes().unwrap();
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

    #[tokio::test]
    async fn test_from_bytes() {
        use crate::UDP_BUFFER_SIZE;
        let input = [
            0x9c, 0x58, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x79,
            0x61, 0x6c, 0x65, 0x6d, 0x61, 0x6e, 0x03, 0x6f, 0x72, 0x67, 0x00, 0x00, 0x01, 0x00,
            0x01,
        ];

        let mut buf: [u8; UDP_BUFFER_SIZE] = [0; UDP_BUFFER_SIZE];
        for (i, b) in input.iter().enumerate() {
            buf[i] = *b as u8;
        }

        let result = crate::parse_query(crate::enums::Protocol::Udp, input.len(), buf, false)
            .await
            .unwrap();

        assert_eq!(
            result.question.as_ref().unwrap().qclass,
            crate::enums::RecordClass::Internet
        );
        assert_eq!(
            result.question.as_ref().unwrap().qname,
            "yaleman.org".as_bytes().to_vec()
        );
        // TODO: make sure *everything* is right here
    }
}

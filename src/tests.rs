#[cfg(test)]
mod tests {

    use crate::ip_address::IPAddress;
    use crate::utils::{convert_u8s_to_u32_be, name_as_bytes};
    use crate::{PacketType, Question, ResourceRecord};
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
        let result: u32 = convert_u8s_to_u32_be(output);
        assert_eq!(result, 16843009);

        let iptest: IPAddress = IPAddress::new(123, 145, 31, 71);
        let output: [u8; 4] = match iptest.pack() {
            Ok(value) => value,
            Err(error) => {
                panic!("{:?}", error)
            }
        };
        let result: u32 = convert_u8s_to_u32_be(output);
        assert_eq!(result, 2073108295);
    }

    #[tokio::test]
    /// This won't work until DNS pointer compression is implemented
    async fn test_build_iana_org_reply() {
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
        eprintln!("question byte length: {}", question_length);
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
            question,
            answers,
            authorities: vec![],
            additional: vec![],
        };
        let reply_bytes: Vec<u8> = reply.as_bytes().unwrap();
        eprintln!("{:?}", reply_bytes);

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
                Some(expected_byte) => eprintln!(
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
        eprintln!("question byte length: {}", question_length);

        let rdata = IPAddress::new(0, 0, 0, 0).pack().unwrap();
        // eprintln!("rdata: {:?}", rdata);
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
            question,
            answers,
            authorities: vec![],
            additional: vec![],
        };
        let reply_bytes: Vec<u8> = reply.as_bytes().unwrap();
        eprintln!("{} bytes: {:?}", reply_bytes.len(), reply_bytes);

        let expected_bytes = [
            /* header - 12 bytes */
            0x3d, 0xe1, 0x85, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            /* question - 16 bytes */
            0x06, 0x61, 0x63, 0x6b, 0x63, 0x64, 0x6e, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01,
            0x00, 0x01, /* answer - 16 bytes  */
            0xC0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00,
            0x00, 0x00,
        ];

        eprintln!("Our length: {}", reply_bytes.len());
        eprintln!("Exp length: {}", expected_bytes.len());

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
                Some(expected_byte) => eprintln!(
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
}

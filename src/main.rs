/*

todo: compression

ok so to point at the name in the question the answer just has to be [0xc0, 0x0c]

ie compression (11000000)
then the 12th octet (0b00001100)
*/

// TODO: SLIST? https://www.rfc-editor.org/rfc/rfc1034 something about state handling.
// TODO: lowercase all question name fields
// TODO: lowercase all reply name fields

// all the types and codes and things - https://www.iana.org/assignments/dns-parameters/dns-parameters.xhtml#dns-parameters-4

use log::{debug, error, info, LevelFilter};
use packed_struct::prelude::*;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use utils::{convert_u32_to_u8s_be, get_config, ConfigFile};

use crate::enums::*;
use crate::ip_address::IPAddress;
use crate::utils::*;

mod enums;
mod ip_address;
mod packet_dumper;
mod tests;
mod utils;

/// builds a servfail response
// async fn reply_servfail(question: Question) -> Reply {
//     Reply{}
// }

const HEADER_BYTES: usize = 12;
const REPLY_TIMEOUT_MS: u64 = 200;

/// https://www.rfc-editor.org/rfc/rfc1035 Section 4.1.1
#[allow(dead_code)]
#[derive(Debug, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "12")]
pub struct Header {
    /// The query ID
    #[packed_field(bits = "0..=15", endian = "msb")]
    id: u16,
    // Is it a query or response
    #[packed_field(bits = "16", ty = "enum")]
    qr: PacketType, // bit 16
    #[packed_field(bits = "17..=20", ty = "enum")]
    opcode: OpCode, // 17-20 actually 4 bits
    #[packed_field(bits = "21")]
    authoritative: bool, // 21
    #[packed_field(bits = "22")]
    truncated: bool, // 22
    // RD - Recursion Desired - this bit may be set in a query and is copied into the response.  If RD is set, it directs the name server to pursue the query recursively. Recursive query support is optional.
    #[packed_field(bits = "23")]
    recursion_desired: bool, // 23
    #[packed_field(bits = "24")]
    recursion_available: bool, // 24
    /// reserved, must be all 0's
    #[packed_field(bits = "25")]
    z: bool, // 25-27 -
    #[packed_field(bits = "26")]
    ad: bool,
    #[packed_field(bits = "27")]
    cd: bool,
    #[packed_field(bits = "28..=31", ty = "enum")]
    rcode: Rcode, // bits 28-31
    /// an unsigned 16 bit integer specifying the number of entries in the question section.
    #[packed_field(bits = "32..=47", endian = "msb")]
    qdcount: u16, // bits 32-47
    /// an unsigned 16 bit integer specifying the number of entries in the answer section.
    #[packed_field(bits = "48..=63", endian = "msb")]
    ancount: u16, // 48-63
    /// an unsigned 16 bit integer specifying the number of name server resource records in the authority records section.
    #[packed_field(bits = "64..=79", endian = "msb")]
    nscount: u16, // 64-79
    /// an unsigned 16 bit integer specifying the number of resource records in the additional records section.
    #[packed_field(bits = "80..=95", endian = "msb")]
    arcount: u16, // 80-95
}

// TODO: probably bin this, because moved to packed struct
/// When you want to parse a request
// impl From<[u8; 12]> for Header {
//     fn from(packets: [u8; 12]) -> Header {
//         let packet_bits = BitVec::from_bytes(&packets);

//         let id = get_query_id(&packets);

//         // ignored in a query
//         let authoritative = false;
//         let truncated = packet_bits.get(22).unwrap();
//         let recursion_desired = packet_bits.get(23).unwrap();
//         // ignored in a query
//         let recursion_available = false;

//         let qdcount = get_u16_from_packets(&packets, 4);
//         // let opcode: OpCode = get_u8_from_bits(&packet_bits,17, 4).into();

//         let opcode: OpCode = ((packets[2] & 0b011100) >> 2).into();

//         Header {
//             id,
//             qr: packet_bits[16].into(),
//             opcode,
//             authoritative,
//             truncated,
//             recursion_desired,
//             recursion_available,
//             // should always be 0
//             z: false,
//             ad: false, // TODO: figure this out
//             cd: false, // TODO: figure this out
//             // ignored in a query
//             rcode: Header::default().rcode,
//             qdcount,
//             // not used in a query
//             ancount: 0,
//             // not used in a query
//             nscount: 0,
//             // not used in a query
//             arcount: 0,
//         }
//     }
// }

// impl Default for Header {
//     fn default() -> Self {
//         Header {
//             id: 0,
//             qr: PacketType::Query,
//             opcode: OpCode::Query,
//             authoritative: false,
//             truncated: false,
//             recursion_desired: false,
//             recursion_available: false,
//             z: false,  // TODO: figure this out
//             ad: false, // TODO: figure this out
//             cd: false, // TODO: figure this out
//             rcode: Rcode::NoError,
//             qdcount: 0,
//             ancount: 0,
//             nscount: 0,
//             arcount: 0,
//         }
//     }
// }

/// Query handler
async fn parse_query(
    _proto: Protocol,
    len: usize,
    buf: [u8; 4096],
    config: ConfigFile<'static>,
) -> Result<Reply, String> {
    if config.capture_packets {
        crate::packet_dumper::dump_bytes(buf[0..len].into()).await;
    }
    // we only want the first 12 bytes for the header
    let mut split_header: [u8; HEADER_BYTES] = [0; HEADER_BYTES];
    split_header.copy_from_slice(&buf[0..HEADER_BYTES]);
    // unpack the header for great justice
    let header = match Header::unpack(&split_header) {
        Ok(value) => value,
        Err(error) => {
            // TODO this should be a SERVFAIL response
            return Err(format!("Failed to parse packet: {:?}", error));
        }
    };
    debug!("Buffer length: {}", len);
    eprintln!("Parsed header: {:?}", header);

    match header.opcode {
        OpCode::Query => {
            let question = Question::from_packets(&buf[HEADER_BYTES..len]).await;
            eprintln!("Resolved question: {:?}", question);

            let question = match question {
                Ok(value) => value,
                Err(error) => {
                    // TODO: this should return a SERVFAIL
                    return Err(error);
                }
            };
            // let answer_rdata = String::from("0.0.0.0");
            let answer_rdata = IPAddress::new(0, 0, 0, 0).pack().unwrap();

            // this is our reply - static until that bit's done
            Ok(Reply {
                header: Header {
                    id: header.id,
                    qr: PacketType::Answer,
                    opcode: header.opcode,
                    authoritative: false, // TODO: are we authoritative
                    truncated: false,     // TODO: work out if it's truncated (ie, UDP)
                    recursion_desired: header.recursion_desired,
                    recursion_available: header.recursion_desired, // TODO: work this out
                    z: false,
                    ad: true, // TODO: decide how the ad flag should be set -  "authentic data" - This requests the server to return whether all of the answer and
                    // authority sections have all been validated as secure according to the security policy of the server. AD=1 indicates that all
                    // records have been validated as secure and the answer is not from a OPT-OUT range. AD=0 indicate that some part of the answer
                    // was insecure or not validated. This bit is set by default.
                    cd: false, // TODO: figure this out -  CD (checking disabled) bit in the query. This requests the server to not perform DNSSEC validation of responses.
                    // rcode: Rcode::NoError, // TODO: this could be something to return if we don't die half way through
                    rcode: Rcode::NoError, // TODO: this could be something to return if we don't die half way through
                    qdcount: 1,
                    ancount: 1, // TODO: work out how many we'll return
                    nscount: 0,
                    arcount: 0,
                },
                question: question.clone(),
                answers: vec![ResourceRecord {
                    name: question.qname,
                    record_type: question.qtype,
                    class: question.qclass,
                    ttl: 60, // TODO: set a TTL
                    rdlength: (answer_rdata.len() as u16),
                    rdata: answer_rdata.to_vec(),
                    compression: true,
                }],
                authorities: vec![],
                additional: vec![],
            })
        }
        _ => {
            // TODO: turn this into a proper packet response
            Err(String::from("Invalid OPCODE"))
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct Reply {
    header: Header,
    question: Question,
    answers: Vec<ResourceRecord>,
    authorities: Vec<ResourceRecord>,
    additional: Vec<ResourceRecord>,
}

impl Reply {
    /// This is used to turn into a series of bytes to yeet back to the client, needs to take a mutable self because the answers record length goes into the header
    fn as_bytes(&mut self) -> Result<Vec<u8>, String> {
        let mut retval: Vec<u8> = vec![];

        self.header.ancount = self.answers.len() as u16;

        // use the packed_struct to build the bytes
        let reply_header = match self.header.pack() {
            Ok(value) => value,
            // TODO: this should not be a panic
            Err(err) => return Err(format!("Failed to pack reply header bytes: {:?}", err)),
        };
        eprintln!("reply_header {:?}", reply_header);
        retval.extend(reply_header);

        // need to add the question in here
        retval.extend(self.question.to_bytes());

        for answer in self.answers.clone() {
            let reply_bytes: Vec<u8> = answer.into();
            retval.extend(reply_bytes);
        }

        Ok(retval)
    }
}

/// The answer, authority, and additional sections all share the same
/// format: a variable number of resource records, where the number of
/// records is specified in the corresponding count field in the header.
///
/// https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.3
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ResourceRecord {
    // NAME            a domain name to which this resource record pertains.
    name: Vec<u8>,

    // TYPE            two octets containing one of the RR type codes.  This
    // field specifies the meaning of the data in the RDATA field.
    record_type: RecordType,
    // CLASS           two octets which specify the class of the data in the RDATA field.
    class: RecordClass,
    // TTL             a 32 bit unsigned integer that specifies the time
    // interval (in seconds) that the resource record may be
    // cached before it should be discarded.  Zero values are
    // interpreted to mean that the RR can only be used for the
    // transaction in progress, and should not be cached.
    ttl: u32,
    // RDLENGTH        an unsigned 16 bit integer that specifies the length in octets of the RDATA field.
    rdlength: u16, // TODO this probably doesn't need to be set, since it can come off the length of rdata

    // TODO: this probably shouldn't be a string, but it is!
    // RDATA           a variable length string of octets that describes the resource.
    // The format of this information varies according to the TYPE and CLASS of the resource record.
    // For example, the if the TYPE is A and the CLASS is IN, the RDATA field is a 4 octet ARPA Internet address.
    rdata: Vec<u8>,
    compression: bool,
}
impl ResourceRecord {}

impl From<ResourceRecord> for Vec<u8> {
    fn from(record: ResourceRecord) -> Self {
        let mut retval: Vec<u8> = vec![];

        eprintln!("{:?}", record);
        // we are compressing for a test
        let record_name_bytes = name_as_bytes(record.name, Some(HEADER_BYTES as u16));
        eprintln!("name_as_bytes: {:?}", record_name_bytes);
        retval.extend(record_name_bytes);
        // type
        retval.extend(convert_u16_to_u8s_be(record.record_type as u16));
        // class
        retval.extend(convert_u16_to_u8s_be(record.class as u16));
        // reply ttl
        let ttl_bytes = convert_u32_to_u8s_be(record.ttl);
        eprintln!("ttl_bytes: {:?}", ttl_bytes);
        retval.extend(ttl_bytes);
        // reply data length
        retval.extend(convert_u16_to_u8s_be(record.rdlength));
        // rdata
        retval.extend(record.rdata);
        // match record.record_type {
        //     RecordType::A => {
        //         let ip_to_int = crate::ip_address::IPAddress::new(1, 2, 3, 4)
        //             .pack()
        //             .unwrap();
        //         // rdata length
        //         retval.extend([0, 4]); // TODO: this is a hack to just yolo a 32 bit address in
        //                                // ip address
        //         retval.extend(ip_to_int);
        //     }
        //     RecordType::NS => todo!(),
        //     RecordType::MD => todo!(),
        //     RecordType::MF => todo!(),
        //     RecordType::CNAME => todo!(),
        //     RecordType::SOA => todo!(),
        //     RecordType::MB => todo!(),
        //     RecordType::MG => todo!(),
        //     RecordType::MR => todo!(),
        //     RecordType::NULL => todo!(),
        //     RecordType::WKS => todo!(),
        //     RecordType::PTR => todo!(),
        //     RecordType::HINFO => todo!(),
        //     RecordType::MINFO => todo!(),
        //     RecordType::MX => todo!(),
        //     RecordType::TXT => todo!(),
        //     RecordType::AAAA => todo!(),
        //     RecordType::AXFR => todo!(),
        //     RecordType::MAILB => todo!(),
        //     RecordType::MAILA => todo!(),
        //     RecordType::ALL => todo!(),
        //     RecordType::InvalidType => todo!(),
        // }
        info!("ResourceRecord Bytes: {:?}", retval);
        retval
    }
}

// TODO: can this be a packed struct for parsing? the qname is a padded string, so it doesn't have a set length
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    qname: Vec<u8>,
    qtype: RecordType,
    qclass: RecordClass,
}

impl Question {
    /// hand it the *actual* length of the buffer and the things, and get back a [Question]
    async fn from_packets(buf: &[u8]) -> Result<Self, String> {
        let mut qname: Vec<u8> = vec![];
        let mut read_pointer = 0;
        let mut next_end = 0;
        let mut in_record_data: bool = false;
        // until we hit a null, read bytes to get the name. I'm sure this won't blow up at any point.
        for qchar in buf.iter().take_while(|b| b != &&0) {
            // eprintln!("p: {}, np: {} {:?} {:?}", read_pointer, next_end, qchar, std::str::from_utf8(&[qchar.to_owned()]).unwrap());
            if read_pointer == next_end {
                in_record_data = false;
                next_end = read_pointer + qchar + 1;
                if read_pointer != 0 {
                    // eprintln!("adding .");
                    qname.push(46);
                }
            } else if in_record_data {
                next_end = read_pointer + qchar;
                in_record_data = true;
            } else {
                qname.push(*qchar);
            }
            read_pointer += 1;
        }
        read_pointer += 2;
        // next byte after the query is the type
        let qtype = &buf[(read_pointer as usize)];
        let qtype: RecordType = qtype.into();
        // next byte after the type is the the class
        let qclass = &buf[(read_pointer as usize) + 2];
        let qclass: RecordClass = qclass.into();

        Ok(Question {
            qname,
            qtype,
            qclass,
        })
    }

    /// turn a question into a vec of bytes to send back to the user
    fn to_bytes(&self) -> Vec<u8> {
        let mut retval: Vec<u8> = vec![];

        let name_bytes = name_as_bytes(self.qname.clone(), None);
        retval.extend(name_bytes);
        retval.extend(convert_u16_to_u8s_be(self.qtype as u16));
        retval.extend(convert_u16_to_u8s_be(self.qclass as u16));
        eprintln!("Question: {:?}", retval);
        retval
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    femme::with_level(LevelFilter::Trace);

    let config: ConfigFile = get_config();

    let listen_addr = format!("{}:{}", config.address, config.port);

    info!("Starting UDP server on {}:{}", config.address, config.port);
    let bind_address = match listen_addr.parse::<SocketAddr>() {
        Ok(value) => value,
        Err(error) => {
            error!("Failed to parse address: {:?}", error);
            return Ok(());
        }
    };
    let udp_sock = match UdpSocket::bind(bind_address).await {
        Ok(value) => value,
        Err(error) => {
            error!("Failed to start UDP listener: {:?}", error);
            return Ok(());
        }
    };

    let mut udp_buffer = [0; 4096];

    loop {
        let (len, addr) = match udp_sock.recv_from(&mut udp_buffer).await {
            Ok(value) => value,
            Err(error) => panic!("{:?}", error),
        };
        debug!("{:?} bytes received from {:?}", len, addr);

        let udp_result = match timeout(
            Duration::from_millis(REPLY_TIMEOUT_MS),
            parse_query(Protocol::Udp, len, udp_buffer, config),
        )
        .await
        {
            Ok(reply) => reply,
            Err(_) => {
                error!("Did not receive response from parse_query within 10 ms");
                continue;
            }
        };

        match udp_result {
            Ok(mut r) => {
                debug!("Result: {:?}", r);

                let reply_bytes: Vec<u8> = match r.as_bytes() {
                    Ok(value) => value,
                    Err(error) => {
                        error!("Failed to parse reply {:?} into bytes: {:?}", r, error);
                        continue;
                    }
                };
                debug!("reply_bytes: {:?}", reply_bytes);
                let len = match udp_sock.send_to(&reply_bytes as &[u8], addr).await {
                    Ok(value) => value,
                    Err(err) => {
                        error!("Failed to send data back to {:?}: {:?}", addr, err);
                        return Ok(());
                    }
                };
                // let len = sock.send_to(r.answer.as_bytes(), addr).await?;
                debug!("{:?} bytes sent", len);
            }
            Err(error) => error!("Error: {}", error),
        }
    }
}

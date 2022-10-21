// TODO: SLIST? <https://www.rfc-editor.org/rfc/rfc1034> something about state handling.
// TODO: lowercase all question name fields
// TODO: lowercase all reply name fields
// TODO: clean ctrl-c handling or shutdown in general

// all the types and codes and things - <https://www.iana.org/assignments/dns-parameters/dns-parameters.xhtml#dns-parameters-4>

extern crate diesel;
#[macro_use]
extern crate lazy_static;

use log::{debug, error, info, trace, LevelFilter};
use packed_struct::prelude::*;
use std::fmt::{Debug, Display};
use std::io;
use std::net::SocketAddr;
use std::str::{from_utf8, FromStr};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::config::{get_config, ConfigFile};
use crate::enums::*;
use crate::reply::Reply;
use crate::utils::*;

pub mod config;
pub mod datastore;
pub mod db;
pub mod enums;
pub mod packet_dumper;
pub mod reply;
pub mod resourcerecord;
pub mod servers;
mod tests;
pub mod utils;
pub mod zones;

const MAX_IN_FLIGHT: usize = 128;
const HEADER_BYTES: usize = 12;
const REPLY_TIMEOUT_MS: u64 = 200;
// <https://dnsflagday.net/2020/#dns-flag-day-2020>
const UDP_BUFFER_SIZE: usize = 1232;

/// <https://www.rfc-editor.org/rfc/rfc1035> Section 4.1.1
#[derive(Debug, PackedStruct, PartialEq, Eq, Clone)]
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

impl Default for Header {
    fn default() -> Self {
        Header {
            id: 0,
            qr: PacketType::Query,
            opcode: OpCode::Query,
            authoritative: false,
            truncated: false,
            recursion_desired: false,
            recursion_available: false,
            z: false,
            ad: false,
            cd: false,
            rcode: Rcode::NoError,
            qdcount: 0,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }
}

impl Header {
    pub fn as_answer(self) -> Header {
        let mut response = self;
        response.qr = PacketType::Answer;
        response
    }
}

/// The answer, authority, and additional sections all share the same
/// format: a variable number of resource records, where the number of
/// records is specified in the corresponding count field in the header.
///
/// <https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.3>
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
    // rdlength: u16, // TODO this probably doesn't need to be set, since it can come off the length of rdata

    // TODO: this probably shouldn't be a string, but it is!
    // RDATA           a variable length string of octets that describes the resource.
    // The format of this information varies according to the TYPE and CLASS of the resource record.
    // For example, the if the TYPE is A and the CLASS is IN, the RDATA field is a 4 octet ARPA Internet address.
    rdata: Vec<u8>,
    // compression: bool,
}
impl ResourceRecord {}

impl From<ResourceRecord> for Vec<u8> {
    fn from(record: ResourceRecord) -> Self {
        Vec::<u8>::from(&record)
    }
}

impl From<&ResourceRecord> for Vec<u8> {
    fn from(record: &ResourceRecord) -> Self {
        let mut retval: Vec<u8> = vec![];

        debug!("{:?}", record);

        let record_name_bytes =
            name_as_bytes(record.name.to_vec(), Some(HEADER_BYTES as u16), None);
        retval.extend(record_name_bytes);
        // type
        retval.extend((record.record_type as u16).to_be_bytes());
        // class
        retval.extend((record.class as u16).to_be_bytes());
        // reply ttl
        let ttl_bytes: [u8; 4] = record.ttl.to_be_bytes();
        trace!("ttl_bytes: {:?}", ttl_bytes);
        retval.extend(ttl_bytes);
        // reply data length
        let rdlength = (record.rdata.len() as u16).to_be_bytes();
        retval.extend(rdlength);
        // retval.extend(record.rdlength.to_be_bytes());
        // rdata
        retval.extend(record.rdata.to_vec());

        #[cfg(debug)]
        for byte in retval.chunks(2) {
            debug!(
                "{:02x} {:02x} {:#010b} {:#010b} {:3} {:3}",
                byte[0], byte[1], byte[0], byte[1], byte[0], byte[1],
            );
        }
        retval
    }
}

// This'd be really nice to be a packed struct
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    qname: Vec<u8>,
    qtype: RecordType,
    qclass: RecordClass,
}

// impl Debug for Question {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.write_str("Question\n")?;
//         for byte in self.to_bytes().chunks(2) {
//             if byte.len() == 2 {
//                 f.write_str(&format!(
//                     "\n{:04x} {:04x} {:#010b} {:#010b} {:3} {:3}",
//                     byte[0], byte[1], byte[0], byte[1], byte[0], byte[1],
//                 ))?;
//             } else {
//                 f.write_str(&format!(
//                     "\n{:04x}      {:#010b}            {:3}",
//                     byte[0], byte[0], byte[0],
//                 ))?;
//             }
//         }
//         f.write_fmt(format_args!(""))
//     }
// }

impl Display for Question {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let qname = match from_utf8(&self.qname) {
            Ok(value) => value.to_string(),
            Err(_) => {
                format!("{:?}", self.qname)
            }
        };
        f.write_fmt(format_args!(
            "QNAME={} QTYPE={:?} QCLASS={}",
            qname, self.qtype, self.qclass,
        ))
    }
}

#[cfg(test)]
mod test {
    use super::Question;
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
}

impl Question {
    fn normalized_name(&self) -> Result<String, String> {
        match from_utf8(&self.qname) {
            Ok(value) => Ok(value.to_lowercase()),
            Err(error) => {
                // TODO: this probably shouldn't be a panic
                Err(format!(
                    "Failed to normalize {:?}: {:?}",
                    &self.qname, error
                ))
            }
        }
    }

    /// hand it the *actual* length of the buffer and the things, and get back a [Question]
    async fn from_packets(buf: &[u8]) -> Result<Self, String> {
        let mut qname: Vec<u8> = vec![];
        let mut read_pointer = 0;
        let mut next_end = 0;
        let mut in_record_data: bool = false;
        // until we hit a null, read bytes to get the name. I'm sure this won't blow up at any point.
        for qchar in buf.iter().take_while(|b| b != && 0) {
            // TODO: refactor this... it's not *great*
            if read_pointer == next_end {
                in_record_data = false;
                next_end = read_pointer + qchar + 1;
                if read_pointer != 0 {
                    trace!("adding .");
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
        let qtype: RecordType = match buf.get(read_pointer as usize) {
            Some(value) => value.into(),
            // TODO: better errors, also log this
            None => return Err("Failed to parse qtype from header".to_string()),
        };
        // next byte after the type is the the class
        // TODO: work out if I'm pulling the wrong thing here, the +2 is weird?
        let qclass: RecordClass = match buf.get((read_pointer as usize) + 2) {
            Some(value) => value.into(),
            // TODO: better errors, also log this
            None => return Err("Failed to parse qclass from header".to_string()),
        };

        Ok(Question {
            qname,
            qtype,
            qclass,
        })
    }

    /// turn a question into a vec of bytes to send back to the user
    fn to_bytes(&self) -> Vec<u8> {
        let mut retval: Vec<u8> = vec![];

        let name_bytes = name_as_bytes(self.qname.clone(), None, None);
        retval.extend(name_bytes);
        retval.extend((self.qtype as u16).to_be_bytes());
        retval.extend((self.qclass as u16).to_be_bytes());
        // debug!("Question Bytes: {:?}", retval);
        retval
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();
    let config: ConfigFile = get_config(clap_results.get_one::<String>("config"));

    let log_level = match LevelFilter::from_str(config.log_level.as_str()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "Failed to parse log level {:?} - {:?}. Reverting to debug",
                config.log_level.as_str(),
                error
            );

            LevelFilter::Debug
        }
    };
    femme::with_level(log_level);
    info!("Configuration: {}", config);
    let listen_addr = format!("{}:{}", config.address, config.port);

    let bind_address = match listen_addr.parse::<SocketAddr>() {
        Ok(value) => value,
        Err(error) => {
            error!("Failed to parse address: {:?}", error);
            return Ok(());
        }
    };

    let tx: mpsc::Sender<crate::datastore::Command>;
    let rx: mpsc::Receiver<crate::datastore::Command>;
    (tx, rx) = mpsc::channel(MAX_IN_FLIGHT);

    let datastore_manager = tokio::spawn(datastore::manager(rx, config.clone()));
    let udpserver = tokio::spawn(servers::udp_server(
        bind_address,
        config.clone(),
        tx.clone(),
    ));
    let tcpserver = tokio::spawn(servers::tcp_server(
        bind_address,
        config.clone(),
        tx.clone(),
    ));

    loop {
        // if any of the servers bail, the server does too.
        if udpserver.is_finished() || tcpserver.is_finished() || datastore_manager.is_finished() {
            return Ok(());
        }
        sleep(std::time::Duration::from_secs(1)).await;
    }
}

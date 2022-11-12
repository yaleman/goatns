#[macro_use]
extern crate lazy_static;

#[cfg(test)]
#[macro_use(defer)]
extern crate scopeguard;

use crate::enums::*;
use crate::utils::*;
use log::trace;
use packed_struct::prelude::*;
use std::fmt::{Debug, Display};
use std::str::from_utf8;

/// Configuration handling for the server
pub mod config;
/// The data-storing backend for zone information and (eventually) caching.
pub mod datastore;
pub mod db;
pub mod enums;
pub mod packet_dumper;
pub mod reply;
pub mod resourcerecord;
pub mod serializers;
pub mod servers;
#[cfg(test)]
mod tests;
pub mod utils;
/// Configuration and management API
pub mod web;
pub mod zones;

/// Internal limit of in-flight requests
pub const MAX_IN_FLIGHT: usize = 512;
/// The size of a DNS request header
pub const HEADER_BYTES: usize = 12;

/// The default "cancel a server response" timeout
pub const REPLY_TIMEOUT_MS: u64 = 1000;
/// The maximum size of a UDP packet <https://dnsflagday.net/2020/#dns-flag-day-2020>
pub const UDP_BUFFER_SIZE: usize = 1232;

pub const COOKIE_NAME: &'static str = "goatns_session";

/// The header of a DNS transmission, either a Query or Reply. Ref [RFC1035](https://www.rfc-editor.org/rfc/rfc1035#section-4.1.1) section 4.1.1.
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
            /// we *are* an authoritative DNS server after all
            authoritative: true,
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
/// Ref [RFC1035 Section 4.1.3](https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.3)
#[derive(Clone, Debug)]
pub struct ResourceRecord {
    /// A domain name to which this resource record pertains.
    pub name: Vec<u8>,
    /// Two octets containing one of the RR type codes. This field specifies the meaning of the data in the RDATA field. The official name is "type".
    pub record_type: RecordType,
    /// Two octets which specify the class of the data in the RDATA field.
    pub class: RecordClass,
    /// A 32 bit unsigned integer that specifies the time interval (in seconds) that the resource record may be cached before it should be discarded. Zero values are interpreted to mean that the RR can only be used for the transaction in progress, and should not be cached.
    pub ttl: u32,
    /// A variable length string of octets that describes the resource.
    /// The format of this information varies according to the TYPE and CLASS of the resource record.
    /// For example, the if the TYPE is A and the CLASS is IN, the RDATA field is a 4 octet ARPA Internet address.
    pub rdata: Vec<u8>,
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

        trace!("{:?}", record);

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
        retval.extend((record.rdata.len() as u16).to_be_bytes());
        // rdata
        retval.extend(record.rdata.to_vec());

        retval
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A DNS Question section, from Ref [RFC1035](https://www.rfc-editor.org/rfc/rfc1035#section-4.1.2) section 4.1.2 "Question section format".
pub struct Question {
    /// The name which is being queried
    qname: Vec<u8>,
    /// The Record type that is being requested, eg A, NS, MX, TXT etc.
    qtype: RecordType,
    /// The class, (typically IN for "Internet")
    qclass: RecordClass,
}

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

/// Returns a Vec<u8> representation of the name
/// (ie, example.com = [101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109])
pub fn get_question_qname(input_val: &[u8]) -> Result<Vec<u8>, String> {
    trace!("got buf: {input_val:?}");
    if input_val.is_empty() {
        return Err("zero-length buffer? That's bad.".to_string());
    }
    // let's make a mut copy of the input so we can do things to do it
    let mut buf = input_val.to_owned();

    let mut result: Vec<u8> = vec![];
    while !buf.is_empty() {
        let label_len = buf[0] as usize;
        if label_len == 0 {
            // we got to the end
            break;
        } else if label_len > 63 {
            return Err(format!("Label length provided was {label_len}, needs to be <=63 while parsing {input_val:?}"));
        }
        if buf.len() < label_len + 1 {
            return Err(format!(
                "Label length was {label_len} but remaining size was too short: {} while parsing {input_val:?}",
                buf.len()
            ));
        }
        #[cfg(test)]
        eprintln!("Before extend: {result:?}");
        if buf.len() < label_len {
            return Err(format!(
                "Buffer was too sort to pull bytes from ({})",
                buf.len()
            ));
        }
        result.extend(buf[1..label_len + 1].to_vec());
        #[cfg(test)]
        eprintln!("After extend:  {result:?}");

        // slice off the front part
        #[cfg(test)]
        eprintln!(
            "Before slicing buf: {buf:?}, about to grab {}..{}",
            label_len + 1,
            buf.len()
        );
        buf = buf[label_len + 1..buf.len()].to_vec();
        trace!("After slicing buf:  {buf:?}");
        if buf[0] != 0 {
            result.push(46);
        }
        if result.len() > 255 {
            return Err(format!(
                "qname length over 255 while parsing question: {result:?}"
            ));
        }
    }
    let result_string = match from_utf8(&result) {
        Ok(value) => value.to_owned().to_lowercase(),
        Err(error) => return Err(format!("{error:?}")),
    };
    #[cfg(test)]
    eprintln!("Returning from get_question_qname {result_string:?}");
    Ok(result_string.as_bytes().to_vec())
}

impl Question {
    fn normalized_name(&self) -> Result<String, String> {
        match from_utf8(&self.qname) {
            Ok(value) => Ok(value.to_lowercase()),
            Err(error) => Err(format!(
                "Failed to normalize {:?}: {:?}",
                &self.qname, error
            )),
        }
    }

    /// hand it the buffer and the things, and get back a [Question]
    async fn from_packets(buf: &[u8]) -> Result<Self, String> {
        let qname = get_question_qname(buf)?;

        // skip past the end of the question
        let read_pointer = qname.len() + 2;
        if buf.len() <= read_pointer + 1 {
            return Err(format!(
                "Packet not long enough, looked for {read_pointer}, got {}",
                buf.len()
            ));
        }
        let mut qtype_bytes: [u8; 2] = [0; 2];
        if buf[read_pointer..read_pointer + 2].len() != 2 {
            return Err(
                "Couldn't get two bytes when I asked for it from the header for the QTYPE"
                    .to_string(),
            );
        }
        qtype_bytes.copy_from_slice(&buf[read_pointer..read_pointer + 2]);
        let qtype = RecordType::from(&u16::from_be_bytes(qtype_bytes));
        let mut qclass_bytes: [u8; 2] = [0; 2];
        if buf.len() <= read_pointer + 3 {
            return Err("Buffer length too short to get two bytes when I asked for it from the header for the QCLASS"
            .to_string(),);
        }
        if buf[read_pointer + 2..read_pointer + 4].len() != 2 {
            return Err(
                "Couldn't get two bytes when I asked for it from the header for the QCLASS"
                    .to_string(),
            );
        }
        qclass_bytes.copy_from_slice(&buf[read_pointer + 2..read_pointer + 4]);
        let qclass: RecordClass = RecordClass::from(&u16::from_be_bytes(qclass_bytes));

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
        retval
    }
}

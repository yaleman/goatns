#[allow(dead_code)]

// use packed_struct::prelude::*;

use bit_vec::{self, BitVec};
use tokio::net::UdpSocket;
use std::io;
use std::net::SocketAddr;

mod utils;

/// parse dat packet
/// https://www.rfc-editor.org/rfc/rfc1035
///
/*
The header contains the following fields:

1  1  1  1  1  1
0  1  2  3  4  5  6  7  8  9  0  1  2  3  4  5
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                      ID                       |
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|QR|   Opcode  |AA|TC|RD|RA|   Z    |   RCODE   |
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    QDCOUNT                    |
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    ANCOUNT                    |
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    NSCOUNT                    |
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    ARCOUNT                    |
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+

where:

ID              A 16 bit identifier assigned by the program that
generates any kind of query.  This identifier is copied
the corresponding reply and can be used by the requester
to match up replies to outstanding queries.

QR              A one bit field that specifies whether this message is a
query (0), or a response (1).

OPCODE          A four bit field that specifies kind of query in this
message.  This value is set by the originator of a query
and copied into the response.  The values are:

0               a standard query (QUERY)

1               an inverse query (IQUERY)

2               a server status request (STATUS)

3-15            reserved for future use

AA              Authoritative Answer - this bit is valid in responses,
and specifies that the responding name server is an
authority for the domain name in question section.

Note that the contents of the answer section may have
multiple owner names because of aliases.  The AA bit
 */

#[allow(dead_code)]
async fn parse_query(len: usize, buf: [u8; 4096]) -> Result<Reply,String> {

    eprintln!("Buffer length: {}", len);
    let header = PacketHeader::from(&buf);

    eprintln!("Parsed header: {:?}", header);

    if header.opcode != OpCode::Query {
        // return Ok(Reply {
        //     answer: format!("Invalid OPCODE: {:?}", header.opcode)
        // })
        return Err(String::from("Invalid OPCODE"))
    }


    Ok(Reply{
        answer: String::from("lol"),
    })
}

// #[derive(Deserialize, Debug)]
// struct QueryUdp {

// }

#[allow(dead_code)]
#[derive(Debug)]
struct Reply {
    answer: String
}


#[derive(Debug)]
enum PacketType {
    Query,
    Answer,
}

impl From<bool> for PacketType {
    fn from(input: bool) -> Self {
        match input {
            false => Self::Query,
            true => Self::Answer,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PacketHeader {
    /// The query ID
    id: u16,
    /// Is it a query or response
    qr: PacketType, // bit 16
    opcode: OpCode, // 17-20 actually 4 bits
    authoritative: bool, // 21
    truncated: bool, // 22


    // RD              Recursion Desired - this bit may be set in a query and
    // is copied into the response.  If RD is set, it directs
    // the name server to pursue the query recursively.
    // Recursive query support is optional.
    recursion_desired: bool, // 23
    recursion_available: bool, // 24
    z: u8, // 25-27 - reserved, must be all 0's
    rcode: Rcode, // bits 28-31
    qdcount: u16, // bits 32-47
    ancount: u16, // 48-63
    nscount: u16, // 64-79
    arcount: u16, // 80-95
}

/// When you want to parse a request
impl From<&[u8; 4096]> for PacketHeader {
    fn from(packets: &[u8; 4096]) -> PacketHeader {
        let packet_bits = BitVec::from_bytes(packets);

        let id = crate::utils::get_query_id(packets);

        // ignored in a query
        let authoritative = false;
        let truncated = packet_bits.get(22).unwrap();
        let recursion_desired = packet_bits.get(23).unwrap();
        // ignored in a query
        let recursion_available = false;

        let qdcount = crate::utils::get_u16_from_packets(packets, 4);
        // let opcode: OpCode = crate::utils::get_u8_from_bits(&packet_bits,17, 4).into();

        let opcode: OpCode = ((packets[2] & 0b011100) >> 2).into();

        PacketHeader {
            id,
            qr: packet_bits[16].into(),
            opcode,
            authoritative,
            truncated,
            recursion_desired,
            recursion_available,
            // should always be 0
            z: 0,
            // ignored in a query
            rcode: PacketHeader::default().rcode,
            qdcount,
            // not used in a query
            ancount: 0,
            // not used in a query
            nscount: 0,
            // not used in a query
            arcount: 0
        }

    }
}

impl Default for PacketHeader {
    fn default() -> Self {
        PacketHeader {
            id: 0,
            qr: PacketType::Query,
            opcode: OpCode::Query,
            authoritative: false,
            truncated: false,
            recursion_desired: false,
            recursion_available: false, z: 0,
            rcode: Rcode::NoError,
            qdcount: 0,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }
}


/// A four bit field that specifies kind of query in this message.
/// This value is set by the originator of a query and copied into the response.
#[derive(Debug, PartialEq)]
pub enum OpCode {
    Query,
    // 0               a standard query (QUERY)
    IQuery,
    // 1               an inverse query (IQUERY)
    Status,
    // 2               a server status request (STATUS)
    Reserved,
    // 3-15            reserved for future use
}

impl From<u8> for OpCode{
    fn from(input: u8) -> Self {
        match input {
            0 => Self::Query,
            1 => Self::IQuery,
            2 => Self::Status,
            _ => Self::Reserved
        }
    }
}

#[derive(Debug)]
pub enum Rcode {
    NoError,
    // 0               No error condition

    FormatError,
    // 1               Format error - The name server was
    //                 unable to interpret the query.

    ServFail,
    // 2               Server failure - The name server was
    //                 unable to process this query due to a
    //                 problem with the name server.

    NameError,
    // 3               Name Error - Meaningful only for
    //                 responses from an authoritative name
    //                 server, this code signifies that the
    //                 domain name referenced in the query does
    //                 not exist.

    NotImplemented,
    // 4               Not Implemented - The name server does
    //                 not support the requested kind of query.

    Refused,
    // 5               Refused - The name server refuses to
    //                 perform the specified operation for
    //                 policy reasons.  For example, a name
    //                 server may not wish to provide the
    //                 information to the particular requester,
    //                 or a name server may not wish to perform
    //                 a particular operation (e.g., zone
    // Reserved,
    // 6-15 - Reserved for future use
}

pub enum RecordType {
    A,	    // 1 a host address
    NS,	    // 2 an authoritative name server
    MD,	    // 3 a mail destination (Obsolete - use MX)
    MF,	    // 4 a mail forwarder (Obsolete - use MX)
    CNAME,	// 5 the canonical name for an alias
    SOA,	// 6 marks the start of a zone of authority
    MB,	    // 7 a mailbox domain name (EXPERIMENTAL)
    MG,	    // 8 a mail group member (EXPERIMENTAL)
    MR,	    // 9 a mail rename domain name (EXPERIMENTAL)
    NULL,	// 10 a null RR (EXPERIMENTAL)
    WKS,	// 11 a well known service description
    PTR,	// 12 a domain name pointer
    HINFO,	// 13 host information
    MINFO,	// 14 mailbox or mail list information
    MX, 	// 15 mail exchange
    TXT,	// 16 text strings
    AXFR,	// 252 A request for a transfer of an entire zone

    MAILB,	// 253 A request for mailbox-related records (MB, MG or MR)

    MAILA,	// 254 A request for mail agent RRs (Obsolete - see MX)

    ALL, // 255 A request for all records (*)
}

impl From<u8> for RecordType {
    fn from(input: u8) -> Self {
        match input{
			1 => Self::A,
			2 => Self::NS,
			3 => Self::MD,
			4 => Self::MF,
			5 => Self::CNAME,
			6 => Self::SOA,
			7 => Self::MB,
			8 => Self::MG,
			9 => Self::MR,
			10 => Self::NULL,
			11 => Self::WKS,
			12 => Self::PTR,
			13 => Self::HINFO,
			14 => Self::MINFO,
			15 => Self::MX,
			16 => Self::TXT,
			252 => Self::AXFR,
			253 => Self::MAILB,
			254 => Self::MAILA,
			255 => Self::ALL,
            // TODO throw some kind of error here
            _ => Self::ALL,

        }

    }
}

pub enum RecordClass {
    // 3.2.4. CLASS values
    // CLASS fields appear in resource records.  The following CLASS mnemonics
    // and values are defined:

    // IN              1 the Internet
    Internet,
    // CS              2 the CSNET class (Obsolete - used only for examples in some obsolete RFCs)
    CsNet,
    // CH              3 the CHAOS class
    Chaos,
    // HS              4 Hesiod [Dyer 87]
    Hesiod,

}

/// The answer, authority, and additional sections all share the same
/// format: a variable number of resource records, where the number of
/// records is specified in the corresponding count field in the header.
#[allow(dead_code)]
pub struct ResourceRecord {
    // NAME            a domain name to which this resource record pertains.
    name: String,

    // TYPE            two octets containing one of the RR type codes.  This
    // field specifies the meaning of the data in the RDATA field.
    // TODO: maybe rename this?
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
    rdlength: u16,

    // TODO: this probably shouldn't be a string, but it is!
    // RDATA           a variable length string of octets that describes the resource.
    // The format of this information varies according to the TYPE and CLASS of the resource record.
    // For example, the if the TYPE is A and the CLASS is IN, the RDATA field is a 4 octet ARPA Internet address.
    rdata: String,
}


/// Pulled from https://docs.rs/tokio/latest/tokio/net/struct.UdpSocket.html#example-one-to-many-bind
#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = "0.0.0.0";
    let port = "15353";

    let sock = UdpSocket::bind(format!("{}:{}", addr, port).parse::<SocketAddr>().unwrap()).await?;
    println!("Listening on {}:{}", addr, port);

    let mut buf = [0; 4096];
    loop {
        let (len, addr) = sock.recv_from(&mut buf).await?;
        println!("{:?} bytes received from {:?}", len, addr);

        let result = parse_query(len, buf).await;
        match result {
            Ok(r) => {
                println!("Result: {:?}", r);
                let len = sock.send_to(&buf[..len], addr).await?;
                // let len = sock.send_to(r.answer.as_bytes(), addr).await?;
                println!("{:?} bytes sent", len);
            },
            Err(error) => eprintln!("Error: {}", error)
        }

    }
}

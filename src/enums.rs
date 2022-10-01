use packed_struct::prelude::*;

#[derive(Debug, Eq, PartialEq, PrimitiveEnum_u8, Copy, Clone)]
/// A four bit field that specifies kind of query in this message.
/// This value is set by the originator of a query and copied into the response.
pub enum OpCode {
    Query = 0,
    // 0               a standard query (QUERY)
    // IQuery = 1, an inverse query (IQUERY) - obsolete in https://www.rfc-editor.org/rfc/rfc3425
    Status = 2,
    // 2               a server status request (STATUS)
    Reserved = 15,
    // 3-15            reserved for future use
}

impl From<u8> for OpCode {
    fn from(input: u8) -> Self {
        match input {
            0 => Self::Query,
            // 1 => Self::IQuery,
            2 => Self::Status,
            _ => Self::Reserved,
        }
    }
}

impl From<OpCode> for i32 {
    fn from(val: OpCode) -> i32 {
        match val {
            OpCode::Query => 0b00,
            // OpCode::IQuery => 0b01,
            OpCode::Status => 0b10,
            //  Self::Reserved
            _ => 0b11,
        }
    }
}

#[derive(PrimitiveEnum_u8, Clone, Copy, Debug, Eq, PartialEq)]
/// Response code, things like NOERROR, FORMATERROR, SERVFAIL etc.
pub enum Rcode {
    NoError = 0,        // 0 - No error condition
    FormatError = 1,    // 1 - Format error - The name server was unable to interpret the query.
    ServFail = 2, // 2 - Server failure - The name server was unable to process this query due to a problem with the name server.
    NameError = 3, // 3 - Name Error - Meaningful only for responses from an authoritative name server, this code signifies that the domain name referenced in the query does not exist.
    NotImplemented = 4, // 4 - Not Implemented - The name server does not support the requested kind of query.
    Refused = 5, // 5 - Refused - The name server refuses to perform the specified operation for policy reasons.  For example, a name server may not wish to provide the information to the particular requester, or a name server may not wish to perform a particular operation (e.g., zone transfers
                 // Reserved,
                 // 6-15 - Reserved for future use
}

// impl From<Rcode> for u8 {
//     fn from(val: Rcode) -> u8 {
//         match val {
//             Rcode::NoError => 0,
//             Rcode::FormatError => 1,
//             Rcode::ServFail => 2,
//             Rcode::NameError => 3,
//             Rcode::NotImplemented => 4,
//             Rcode::Refused => 5,
//         }
//     }
// }

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordType {
    A = 1,      // 1 a host address
    NS = 2,     // 2 an authoritative name server
    MD = 3,     // 3 a mail destination (Obsolete - use MX)
    MF = 4,     // 4 a mail forwarder (Obsolete - use MX)
    CNAME = 5,  // 5 the canonical name for an alias
    SOA = 6,    // 6 marks the start of a zone of authority
    MB = 7,     // 7 a mailbox domain name (EXPERIMENTAL)
    MG = 8,     // 8 a mail group member (EXPERIMENTAL)
    MR = 9,     // 9 a mail rename domain name (EXPERIMENTAL)
    NULL = 10,  // 10 a null RR (EXPERIMENTAL)
    WKS = 11,   // 11 a well known service description
    PTR = 12,   // 12 a domain name pointer
    HINFO = 13, // 13 host information
    MINFO = 14, // 14 mailbox or mail list information
    MX = 15,    // 15 mail exchange
    TXT = 16,   // 16 text strings
    AAAA = 28,  // 28 https://www.rfc-editor.org/rfc/rfc3596#section-2.1
    AXFR = 252, // 252 A request for a transfer of an entire zone

    MAILB = 253, // 253 A request for mailbox-related records (MB, MG or MR)

    MAILA = 254, // 254 A request for mail agent RRs (Obsolete - see MX)

    ALL = 255, // 255 A request for all records (*)
    InvalidType,
}

impl From<&u8> for RecordType {
    fn from(input: &u8) -> Self {
        match input {
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
            28 => Self::AAAA, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            252 => Self::AXFR,
            253 => Self::MAILB,
            254 => Self::MAILA,
            255 => Self::ALL,
            _ => Self::InvalidType,
        }
    }
}

impl From<&u16> for RecordType {
    fn from(input: &u16) -> Self {
        match input {
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
            28 => Self::AAAA, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            252 => Self::AXFR,
            253 => Self::MAILB,
            254 => Self::MAILA,
            255 => Self::ALL,
            _ => Self::InvalidType,
        }
    }
}

impl RecordType {
    pub fn supported(self: RecordType) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match self {
            RecordType::A => true,
            RecordType::AAAA => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 3.2.4. CLASS values
/// CLASS fields appear in resource records.
pub enum RecordClass {
    // IN              1 the Internet
    Internet = 1,
    // CS              2 the CSNET class (Obsolete - used only for examples in some obsolete RFCs)
    CsNet = 2,
    // CH              3 the CHAOS class
    Chaos = 3,
    // HS              4 Hesiod [Dyer 87]
    Hesiod = 4,

    InvalidType = 0,
}

impl From<&u8> for RecordClass {
    fn from(input: &u8) -> Self {
        match input {
            1 => Self::Internet,
            2 => Self::CsNet,
            3 => Self::Chaos,
            4 => Self::Hesiod,
            _ => Self::InvalidType,
        }
    }
}

pub enum Protocol {
    // Tcp,
    Udp,
}

#[derive(Debug, PrimitiveEnum_u8, Clone, Copy)]
pub enum PacketType {
    Query = 0,
    Answer = 1,
}

impl From<bool> for PacketType {
    fn from(input: bool) -> Self {
        match input {
            false => Self::Query,
            true => Self::Answer,
        }
    }
}

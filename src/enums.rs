use std::fmt::Display;

use packed_struct::prelude::*;

use crate::resourcerecord::InternalResourceRecord;

#[derive(Debug, Eq, PartialEq, PrimitiveEnum_u8, Copy, Clone)]
/// A four bit field that specifies kind of query in this message.
/// This value is set by the originator of a query and copied into the response.
pub enum OpCode {
    /// A standard query (QUERY)
    Query = 0,
    // IQuery = 1, an inverse query (IQUERY) - obsolete in https://www.rfc-editor.org/rfc/rfc3425
    /// Server status request (STATUS)
    Status = 2,
    /// 3-15            reserved for future use
    Reserved = 15,
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
    // No error condition
    NoError = 0,
    // Format error - The name server was unable to interpret the query.
    FormatError = 1,
    // Server failure - The name server was unable to process this query due to a problem with the name server.
    ServFail = 2,
    /// Name Error - Meaningful only for responses from an authoritative name server, this code signifies that the domain name referenced in the query does not exist.
    NameError = 3,
    /// Not Implemented - The name server does not support the requested kind of query.
    NotImplemented = 4,
    /// Refused - The name server refuses to perform the specified operation for policy reasons.  For example, a name server may not wish to provide the information to the particular requester, or a name server may not wish to perform a particular operation (e.g., zone transfers
    Refused = 5,
    // Reserved,
    // 6..15 - Reserved for future use
}

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
    /// 16 text strings
    TXT = 16,
    /// 28 https://www.rfc-editor.org/rfc/rfc3596#section-2.1
    AAAA = 28,

    /// NAPTR <https://www.rfc-editor.org/rfc/rfc2915>
    NAPTR = 35,
    /// 252 A request for a transfer of an entire zone
    AXFR = 252,
    /// 253 A request for mailbox-related records (MB, MG or MR)
    MAILB = 253,

    /// 255 A request for all records (*)
    ALL = 255,
    /// Certification Authority Restriction - <https://www.rfc-editor.org/rfc/rfc6844.txt>
    CAA = 257,
    InvalidType,
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
            28 => Self::AAAA,  // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            35 => Self::NAPTR, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            252 => Self::AXFR,
            253 => Self::MAILB,
            255 => Self::ALL,
            257 => Self::CAA,
            _ => Self::InvalidType,
        }
    }
}

impl From<String> for RecordType {
    fn from(input: String) -> Self {
        let input: RecordType = input.as_str().into();
        input
    }
}

impl From<&str> for RecordType {
    fn from(input: &str) -> Self {
        match input {
            "A" => Self::A,
            "NS" => Self::NS,
            "MD" => Self::MD,
            "MF" => Self::MF,
            "CNAME" => Self::CNAME,
            "SOA" => Self::SOA,
            "MB" => Self::MB,
            "MG" => Self::MG,
            "MR" => Self::MR,
            "NULL" => Self::NULL,
            "WKS" => Self::WKS,
            "PTR" => Self::PTR,
            "HINFO" => Self::HINFO,
            "MINFO" => Self::MINFO,
            "MX" => Self::MX,
            "TXT" => Self::TXT,
            "AAAA" => Self::AAAA, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            "NAPTR" => Self::NAPTR,
            "AXFR" => Self::AXFR,
            "MAILB" => Self::MAILB,
            "ALL" => Self::ALL,
            _ => Self::InvalidType,
        }
    }
}

impl From<InternalResourceRecord> for RecordType {
    fn from(input: InternalResourceRecord) -> Self {
        match input {
            InternalResourceRecord::A { .. } => RecordType::A,
            InternalResourceRecord::NS { .. } => RecordType::NS,
            InternalResourceRecord::MD { .. } => RecordType::MD,
            InternalResourceRecord::MF { .. } => RecordType::MF,
            InternalResourceRecord::CNAME { .. } => RecordType::CNAME,
            InternalResourceRecord::SOA { .. } => RecordType::SOA,
            InternalResourceRecord::MB { .. } => RecordType::MB,
            InternalResourceRecord::MG { .. } => RecordType::MG,
            InternalResourceRecord::MR { .. } => RecordType::MR,
            InternalResourceRecord::NULL { .. } => RecordType::NULL,
            InternalResourceRecord::WKS { .. } => RecordType::WKS,
            InternalResourceRecord::PTR { .. } => RecordType::PTR,
            InternalResourceRecord::HINFO { .. } => RecordType::HINFO,
            InternalResourceRecord::MINFO { .. } => RecordType::MINFO,
            InternalResourceRecord::MX { .. } => RecordType::MX,
            InternalResourceRecord::TXT { .. } => RecordType::TXT,
            InternalResourceRecord::AAAA { .. } => RecordType::AAAA,
            InternalResourceRecord::AXFR { .. } => RecordType::AXFR,
            InternalResourceRecord::MAILB { .. } => RecordType::MAILB,
            InternalResourceRecord::NAPTR { .. } => RecordType::NAPTR,
            // InternalResourceRecord::MAILA { .. } => RecordType::MAILA,
            InternalResourceRecord::ALL { .. } => RecordType::ALL,
            InternalResourceRecord::InvalidType => RecordType::InvalidType,
            InternalResourceRecord::CAA { .. } => RecordType::CAA,
        }
    }
}

impl RecordType {
    pub fn supported(self: RecordType) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match self {
            RecordType::A => true,
            RecordType::AAAA => true,
            RecordType::CAA => true,
            RecordType::CNAME => true,
            RecordType::HINFO => true,
            RecordType::MX => true,
            RecordType::PTR => true,
            RecordType::SOA => true,
            RecordType::TXT => true,
            RecordType::NS => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 3.2.4. CLASS values
/// CLASS fields appear in resource records.
pub enum RecordClass {
    /// IN - 1 the Internet
    Internet = 1,
    /// CS - 2 the CSNET class (Obsolete - used only for examples in some obsolete RFCs)
    CsNet = 2,
    /// CH              3 the CHAOS class
    Chaos = 3,
    /// HS              4 Hesiod [Dyer 87]
    Hesiod = 4,

    InvalidType = 0,
}

impl Display for RecordClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}",
            match self {
                RecordClass::Internet => "IN",
                RecordClass::CsNet => "CS",
                RecordClass::Chaos => "CHAOS",
                RecordClass::Hesiod => "HESIOD",
                RecordClass::InvalidType => "Invalid",
            }
        ))
    }
}

impl From<&u16> for RecordClass {
    fn from(input: &u16) -> Self {
        match input {
            1 => Self::Internet,
            2 => Self::CsNet,
            3 => Self::Chaos,
            4 => Self::Hesiod,
            _ => Self::InvalidType,
        }
    }
}

#[derive(Debug, PrimitiveEnum_u8, Clone, Copy, Eq, PartialEq)]
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

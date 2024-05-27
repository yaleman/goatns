use crate::resourcerecord::InternalResourceRecord;
use enum_iterator::Sequence;
use packed_struct::prelude::*;
use serde::{de, Serialize, Serializer};
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Agent {
    Datastore,
    API,
    UDPServer,
    TCPServer,
}

#[derive(Clone, Debug)]
pub enum AgentState {
    Started { agent: Agent },
    Stopped { agent: Agent },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SystemState {
    Import,
    Export,
    Server,
    ShuttingDown,
}

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
            2 => Self::Status,
            _ => Self::Reserved,
        }
    }
}

impl From<OpCode> for i32 {
    fn from(val: OpCode) -> i32 {
        match val {
            OpCode::Query => 0b00,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Sequence, sqlx::Type)]
/// RRType, eg A, NS, MX, etc
#[sqlx(type_name = "INTEGER")]
#[repr(i64)]
pub enum RecordType {
    /// A host address
    A = 1,
    /// Authoritative name server
    NS = 2,
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
    /// Text strings
    TXT = 16,
    /// IPv6 Records <https://www.rfc-editor.org/rfc/rfc3596#section-2.1>
    AAAA = 28,
    /// For when you want to know the physical location of a thing! <https://www.rfc-editor.org/rfc/rfc1876>
    LOC = 29,
    /// NAPTR <https://www.rfc-editor.org/rfc/rfc2915>
    NAPTR = 35,
    /// 252 A request for a transfer of an entire zone
    AXFR = 252,
    /// 253 A request for mailbox-related records (MB, MG or MR)
    MAILB = 253,
    URI = 256,
    /// 255 A request for all records (*)
    ANY = 255,
    /// Certification Authority Restriction - <https://www.rfc-editor.org/rfc/rfc6844.txt>
    CAA = 257,
    InvalidType,
}

impl From<&u16> for RecordType {
    fn from(input: &u16) -> Self {
        match input {
            1 => Self::A,
            2 => Self::NS,
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
            29 => Self::LOC,
            35 => Self::NAPTR, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            252 => Self::AXFR,
            253 => Self::MAILB,
            255 => Self::ANY,
            256 => Self::URI,
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
            "AAAA" => Self::AAAA, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
            "ANY" => Self::ANY,
            "AXFR" => Self::AXFR,
            "CAA" => Self::CAA,
            "CNAME" => Self::CNAME,
            "HINFO" => Self::HINFO,
            "LOC" => Self::LOC,
            "MAILB" => Self::MAILB,
            "MB" => Self::MB,
            "MG" => Self::MG,
            "MINFO" => Self::MINFO,
            "MR" => Self::MR,
            "MX" => Self::MX,
            "NAPTR" => Self::NAPTR,
            "NS" => Self::NS,
            "NULL" => Self::NULL,
            "PTR" => Self::PTR,
            "SOA" => Self::SOA,
            "TXT" => Self::TXT,
            "URI" => Self::URI,
            "WKS" => Self::WKS,
            _ => Self::InvalidType,
        }
    }
}

impl From<RecordType> for &'static str {
    fn from(input: RecordType) -> &'static str {
        match input {
            RecordType::A => "A",
            RecordType::AAAA => "AAAA",
            RecordType::ANY => "ANY",
            RecordType::AXFR => "AXFR",
            RecordType::CAA => "CAA",
            RecordType::CNAME => "CNAME",
            RecordType::HINFO => "HINFO",
            RecordType::LOC => "LOC",
            RecordType::MAILB => "MAILB",
            RecordType::MB => "MB",
            RecordType::MG => "MG",
            RecordType::MINFO => "MINFO",
            RecordType::MR => "MR",
            RecordType::MX => "MX",
            RecordType::NAPTR => "NAPTR",
            RecordType::NS => "NS",
            RecordType::NULL => "NULL",
            RecordType::PTR => "PTR",
            RecordType::SOA => "SOA",
            RecordType::TXT => "TXT",
            RecordType::URI => "URI",
            RecordType::WKS => "WKS",
            RecordType::InvalidType => "",
        }
    }
}

impl Display for RecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let res: &'static str = self.to_owned().into();
        f.write_fmt(format_args!("{res}"))
    }
}

impl From<InternalResourceRecord> for RecordType {
    fn from(input: InternalResourceRecord) -> RecordType {
        match input {
            InternalResourceRecord::A { .. } => RecordType::A,
            InternalResourceRecord::AAAA { .. } => RecordType::AAAA,
            InternalResourceRecord::AXFR { .. } => RecordType::AXFR,
            InternalResourceRecord::CAA { .. } => RecordType::CAA,
            InternalResourceRecord::CNAME { .. } => RecordType::CNAME,
            InternalResourceRecord::HINFO { .. } => RecordType::HINFO,
            InternalResourceRecord::InvalidType => RecordType::InvalidType,
            InternalResourceRecord::LOC { .. } => RecordType::LOC,
            InternalResourceRecord::MX { .. } => RecordType::MX,
            InternalResourceRecord::NAPTR { .. } => RecordType::NAPTR,
            InternalResourceRecord::NS { .. } => RecordType::NS,
            InternalResourceRecord::PTR { .. } => RecordType::PTR,
            InternalResourceRecord::SOA { .. } => RecordType::SOA,
            InternalResourceRecord::TXT { .. } => RecordType::TXT,
            InternalResourceRecord::URI { .. } => RecordType::URI,
        }
    }
}

impl RecordType {
    pub fn supported(self: RecordType) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match self {
            RecordType::A
            | RecordType::AAAA
            | RecordType::ANY
            | RecordType::CAA
            | RecordType::CNAME
            | RecordType::HINFO
            | RecordType::LOC
            | RecordType::MX
            | RecordType::NS
            | RecordType::PTR
            | RecordType::SOA
            | RecordType::TXT
            | RecordType::URI => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Sequence, sqlx::Type)]
#[repr(i64)]
/// CLASS fields appear in resource records, most entries should be IN, but CHAOS is typically used for management-layer things. Ref RFC1035 3.2.4.
pub enum RecordClass {
    /// IN - Internet
    Internet = 1,
    /// CS - CSNET class (Obsolete - used only for examples in some obsolete RFCs)
    CsNet = 2,
    /// CH - Chaos
    Chaos = 3,
    /// Hesiod [Dyer 87]
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

impl<'de> de::Deserialize<'de> for RecordClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        Ok(RecordClass::from(s.as_str()))
        // log::trace!("deser input='{}' result='{:?}'", s, res);
        // match res {
        //     Ok(val) => Ok(val),
        //     Err(err) => match err {
        //         crate::enums::ContactDetailsDeserializerError::InputLengthWrong { msg, len } => {
        //             Err(de::Error::invalid_length(len, &msg))
        //         }
        //         crate::enums::ContactDetailsDeserializerError::InputFormatWrong { unexp, exp } => {
        //             Err(de::Error::invalid_value(de::Unexpected::Str(&unexp), &exp))
        //         }
        //         crate::enums::ContactDetailsDeserializerError::WrongContactType(_msg) => {
        //             todo!()
        //         }
        //     },
        // }
    }
}
impl From<&str> for RecordClass {
    fn from(value: &str) -> Self {
        match value {
            "IN" => RecordClass::Internet,
            "CS" => RecordClass::CsNet,
            "CHAOS" => RecordClass::Chaos,
            "HESIOD" => RecordClass::Hesiod,
            _ => RecordClass::InvalidType,
        }
    }
}

impl Serialize for RecordClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{self}").as_str())
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ContactDetails {
    Mastodon { contact: String, server: String },
    Email { contact: String },
    Twitter { contact: String },
    None,
}

impl Default for ContactDetails {
    fn default() -> Self {
        Self::None
    }
}

impl std::fmt::Display for ContactDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContactDetails::Mastodon { server, contact } => f.write_fmt(format_args!(
                r#"<a href="https://{}/@{}">{}</a>"#,
                server, contact, contact
            )),
            ContactDetails::Email { contact } => f.write_fmt(format_args!(
                r#"<a href="mailto:{}">{}</a>"#,
                contact, contact
            )),
            ContactDetails::Twitter { contact } => f.write_fmt(format_args!(
                r#"<a href="https://twitter.com/{}">{}</a>"#,
                contact, contact
            )),
            ContactDetails::None => f.write_str(""),
        }
    }
}
impl ContactDetails {
    /// returns (username, url)
    pub fn to_html_parts(&self) -> (String, String) {
        match self {
            ContactDetails::Mastodon { server, contact } => (
                contact.to_owned(),
                format!("https://{}/@{}", server, contact),
            ),
            ContactDetails::Email { contact } => {
                (contact.to_owned(), format!("mailto:{}", contact))
            }
            ContactDetails::Twitter { contact } => (
                contact.to_owned(),
                format!("https://twitter.com/{}", contact),
            ),
            ContactDetails::None => ("".to_string(), "".to_string()),
        }
    }
}

#[derive(Debug)]
/// Errors returned from trying to turn a String into a [ContactDetails]
pub enum ContactDetailsDeserializerError {
    InputLengthWrong { msg: &'static str, len: usize },
    InputFormatWrong { unexp: String, exp: &'static str },
    WrongContactType(String),
}

impl TryFrom<String> for ContactDetails {
    type Error = ContactDetailsDeserializerError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut split_value = value.split(':');
        let contact_type = split_value.next();
        let contact_value = split_value.next();

        match (contact_type, contact_value) {
            (Some(contact_type), Some(contact_value)) => {
                // return Err(ContactDetailsDeserializerError::InputLengthWrong{
                //     msg: "Length of input is wrong please ensure it's in the format type:username@server (server for Mastodon)",
                //     len: split_value.count(),
                // });

                match contact_type {
                    "Mastodon" => {
                        let contact_value = match contact_value.starts_with('@') {
                            false => contact_value,
                            true => contact_value.trim_start_matches(
                                '@'
                            )
                        };
                        if !contact_value.contains('@') {
                            Err(ContactDetailsDeserializerError::InputFormatWrong{unexp: contact_value.to_string(),exp: "Input format is wrong please ensure it's in the format Mastodon:username@server for Mastodon",
                        })
                        }

                        else {
                            let mut contact_split = contact_value.split('@');

                            Ok( Self::Mastodon {
                                contact: contact_split.next().expect("THe length was checked and then we couldn't get it!").to_string(),
                                server: contact_split.next().expect("THe length was checked and then we couldn't get it!").to_string(),
                            })
                        }

                    },
                    "Email" => {
                        Ok(Self::Email { contact: contact_value.to_string() })

                    },
                    "Twitter" => {
                        Ok(Self::Twitter { contact: contact_value.to_string() })

                    },
                    &_ => {
                        Err(ContactDetailsDeserializerError::WrongContactType(format!(
                                "Contact type ({}) wrong, please ensure it's in the format type:value where type is one of Email/Twitter/Mastodon",
                                contact_type
                                ) ))
                    }
                }
            }
            _ => {
                Err(ContactDetailsDeserializerError::InputLengthWrong{
                    msg: "Length/value of input is wrong. please ensure it's in the format type:username@server (server for Mastodon)",
                    len: split_value.count(),
                })
            }
        }
    }
}

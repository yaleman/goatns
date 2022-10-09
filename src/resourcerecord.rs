use crate::enums::RecordType;
use crate::utils;
use log::*;
use std::env::consts;

use std::str::{from_utf8, FromStr};
// use packed_struct::*;
use crate::rdata::DomainName;
use crate::zones::FileZoneRecord;

// impl From<&u8> for ResourceRecord {
//     fn from(input: &u8) -> Self {
//         match input {
//             1 => Self::A,
//             2 => Self::NS,
//             3 => Self::MD,
//             4 => Self::MF,
//             5 => Self::CNAME,
//             6 => Self::SOA,
//             7 => Self::MB,
//             8 => Self::MG,
//             9 => Self::MR,
//             10 => Self::NULL,
//             11 => Self::WKS,
//             12 => Self::PTR,
//             13 => Self::HINFO,
//             14 => Self::MINFO,
//             15 => Self::MX,
//             16 => Self::TXT,
//             28 => Self::AAAA, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
//             252 => Self::AXFR,
//             253 => Self::MAILB,
//             254 => Self::MAILA,
//             255 => Self::ALL,
//             _ => Self::InvalidType,
//         }
//     }
// }

// impl From<&u16> for ResourceRecord {
//     fn from(input: &u16) -> Self {
//         match input {
//             1 => Self::A,
//             2 => Self::NS,
//             3 => Self::MD,
//             4 => Self::MF,
//             5 => Self::CNAME,
//             6 => Self::SOA,
//             7 => Self::MB,
//             8 => Self::MG,
//             9 => Self::MR,
//             10 => Self::NULL,
//             11 => Self::WKS,
//             12 => Self::PTR,
//             13 => Self::HINFO,
//             14 => Self::MINFO,
//             15 => Self::MX,
//             16 => Self::TXT,
//             28 => Self::AAAA, // https://www.rfc-editor.org/rfc/rfc3596#section-2.1
//             252 => Self::AXFR,
//             253 => Self::MAILB,
//             254 => Self::MAILA,
//             255 => Self::ALL,
//             _ => Self::InvalidType,
//         }
//     }
// }

// impl From<String> for ResourceRecord {
//     fn from(input: String) -> Self {
//         let input: ResourceRecord = input.as_str().into();
//         input
//     }
// }

// impl From<&str> for ResourceRecord {
//     fn from(input: &str) -> Self {
//         match input {
//             "A" => Self::A,
//             "NS" => Self::NS,
//             "MD" => Self::MD,
//             "MF" => Self::MF,
//             "CNAME" => Self::CNAME,
//             "SOA" => Self::SOA,
//             "MB" => Self::MB,
//             "MG" => Self::MG,
//             "MR" => Self::MR,
//             "NULL" => Self::NULL,
//             "WKS" => Self::WKS,
//             "PTR" => Self::PTR,
//             "HINFO" => Self::HINFO,
//             "MINFO" => Self::MINFO,
//             "MX" => Self::MX,
//             "TXT" => Self::TXT,
//             "AAAA" => Self::AAAA,
//             "AXFR" => Self::AXFR,
//             "MAILB" => Self::MAILB,
//             "MAILA" => Self::MAILA,
//             "ALL" => Self::ALL,
//             _ => Self::InvalidType,
//         }
//     }
// }

/// <character-string> is a single length octet followed by that number of characters.  <character-string> is treated as binary information, and can be up to 256 characters in length (including the length octet).
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct DNSCharString {
    pub data: Vec<u8>,
}

impl From<&str> for DNSCharString {
    fn from(input: &str) -> Self {
        DNSCharString { data: input.into() }
    }
}

impl From<DNSCharString> for Vec<u8> {
    fn from(input: DNSCharString) -> Vec<u8> {
        let mut data: Vec<u8> = vec![input.data.len() as u8];
        data.extend(input.data);
        data
    }
}

#[allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum InternalResourceRecord {
    A {
        address: u32,
        ttl: Option<u32>,
    }, // 1 a host address
    NS {
        nsdname: DomainName,
        ttl: Option<u32>,
    }, // 2 an authoritative name server
    MD {
        ttl: Option<u32>,
    }, // 3 a mail destination (Obsolete - use MX)
    MF {
        ttl: Option<u32>,
    }, // 4 a mail forwarder (Obsolete - use MX)
    CNAME {
        cname: DomainName,
        ttl: Option<u32>,
    }, // 5 the canonical name for an alias
    SOA {
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
        // this doesn't get a TTL, since that's an expire?
    }, // 6 marks the start of a zone of authority
    MB {
        ttl: Option<u32>,
    }, // 7 a mailbox domain name (EXPERIMENTAL)
    MG {
        ttl: Option<u32>,
    }, // 8 a mail group member (EXPERIMENTAL)
    MR {
        ttl: Option<u32>,
    }, // 9 a mail rename domain name (EXPERIMENTAL)
    NULL {
        ttl: Option<u32>,
    }, // 10 a null RR (EXPERIMENTAL)
    WKS {
        ttl: Option<u32>,
    }, // 11 a well known service description
    PTR {
        ptrdname: DomainName,
        ttl: Option<u32>,
    }, // 12 a domain name pointer
    HINFO {
        cpu: Option<DNSCharString>,
        os: Option<DNSCharString>,
        ttl: Option<u32>,
    }, // 13 host information
    MINFO {
        ttl: Option<u32>,
    }, // 14 mailbox or mail list information
    MX {
        preference: u16,
        exchange: DomainName,
        ttl: Option<u32>,
    }, // 15 mail exchange
    TXT {
        txtdata: DNSCharString,
        ttl: Option<u32>,
    }, // 16 text strings
    AAAA {
        address: u128,
        ttl: Option<u32>,
    }, // 28 https://www.rfc-editor.org/rfc/rfc3596#section-2.1
    AXFR {
        ttl: Option<u32>,
    }, // 252 A request for a transfer of an entire zone

    MAILB {
        ttl: Option<u32>,
    }, // 253 A request for mailbox-related records (MB, MG or MR)

    MAILA {
        ttl: Option<u32>,
    }, // 254 A request for mail agent RRs (Obsolete - see MX)

    ALL {}, // 255 A request for all records (*)
    InvalidType,
}

impl From<FileZoneRecord> for InternalResourceRecord {
    /// This is where we convert from the JSON blob in the file to an internal representation of the data.
    fn from(record: FileZoneRecord) -> Self {
        match record.rrtype.as_str() {
            "A" => {
                let address = match from_utf8(&record.rdata) {
                    Ok(value) => value,
                    Err(error) => {
                        error!(
                            "Failed to parse {:?} to string in A record: {:?}",
                            record.rdata, error
                        );
                        return InternalResourceRecord::InvalidType;
                    }
                };
                let address: u32 = match std::net::Ipv4Addr::from_str(address) {
                    Ok(value) => value.into(),
                    Err(error) => {
                        error!(
                            "Failed to parse {:?} into an IPv4 address: {:?}",
                            record.rdata, error
                        );
                        0u32
                    }
                };
                InternalResourceRecord::A {
                    address,
                    ttl: record.ttl,
                }
            }
            "AAAA" => {
                let address = match from_utf8(&record.rdata) {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!(
                            "Failed to parse {:?} to string in A record: {:?}",
                            record.rdata, error
                        );
                        return InternalResourceRecord::InvalidType;
                    }
                };
                let address: u128 = match std::net::Ipv6Addr::from_str(address) {
                    Ok(value) => {
                        let res: u128 = value.into();
                        eprintln!("Encoding {:?} as {:?}", value, res);
                        res
                    }
                    Err(error) => {
                        eprintln!(
                            "Failed to parse {:?} into an IPv6 address: {:?}",
                            record.rdata, error
                        );
                        return InternalResourceRecord::InvalidType;
                    }
                };

                InternalResourceRecord::AAAA {
                    address,
                    ttl: record.ttl,
                }
            }
            "TXT" => InternalResourceRecord::TXT {
                txtdata: DNSCharString { data: record.rdata },
                ttl: record.ttl,
            },
            _ => InternalResourceRecord::InvalidType,
        }
    }
}

impl PartialEq<RecordType> for InternalResourceRecord {
    fn eq(&self, other: &RecordType) -> bool {
        match self {
            InternalResourceRecord::A { address: _, ttl: _ } => other == &RecordType::A,
            InternalResourceRecord::AAAA { address: _, ttl: _ } => other == &RecordType::AAAA,
            InternalResourceRecord::ALL {} => other == &RecordType::ALL,
            InternalResourceRecord::AXFR { ttl: _ } => other == &RecordType::AXFR,
            InternalResourceRecord::CNAME { cname: _, ttl: _ } => other == &RecordType::CNAME,
            InternalResourceRecord::HINFO {
                cpu: _,
                os: _,
                ttl: _,
            } => other == &RecordType::HINFO,
            InternalResourceRecord::InvalidType => other == &RecordType::InvalidType,
            InternalResourceRecord::MAILA { ttl: _ } => other == &RecordType::MAILA,
            InternalResourceRecord::MAILB { ttl: _ } => other == &RecordType::MAILB,
            InternalResourceRecord::MB { ttl: _ } => other == &RecordType::MB,
            InternalResourceRecord::MD { ttl: _ } => other == &RecordType::MD,
            InternalResourceRecord::MF { ttl: _ } => other == &RecordType::MF,
            InternalResourceRecord::MG { ttl: _ } => other == &RecordType::MG,
            InternalResourceRecord::MINFO { ttl: _ } => other == &RecordType::MINFO,
            InternalResourceRecord::MR { ttl: _ } => other == &RecordType::MR,
            InternalResourceRecord::MX {
                preference: _,
                exchange: _,
                ttl: _,
            } => other == &RecordType::MX,
            InternalResourceRecord::NS { nsdname: _, ttl: _ } => other == &RecordType::NS,
            InternalResourceRecord::NULL { ttl: _ } => other == &RecordType::NULL,
            InternalResourceRecord::PTR {
                ptrdname: _,
                ttl: _,
            } => other == &RecordType::PTR,
            InternalResourceRecord::SOA {
                serial: _,
                refresh: _,
                retry: _,
                expire: _,
                minimum: _,
            } => other == &RecordType::SOA,
            InternalResourceRecord::TXT { txtdata: _, ttl: _ } => other == &RecordType::TXT,
            InternalResourceRecord::WKS { ttl: _ } => other == &RecordType::WKS,
        }
    }
}

impl InternalResourceRecord {
    pub fn as_bytes(self: InternalResourceRecord) -> Vec<u8> {
        match self {
            InternalResourceRecord::A { address, ttl: _ } => address.to_be_bytes().to_vec(),
            InternalResourceRecord::AAAA { address, ttl: _ } => address.to_be_bytes().to_vec(),
            InternalResourceRecord::TXT { txtdata, ttl: _ } => {
                // <character-string> is a single length octet followed by that number of characters.  <character-string> is treated as binary information, and can be up to 256 characters in length (including the length octet).
                let mut res: Vec<u8> = txtdata.into();
                res.truncate(256);
                res
            }
            // InternalResourceRecord::NS { nsdname } => todo!(),
            // InternalResourceRecord::MD {  } => todo!(),
            // InternalResourceRecord::MF {  } => todo!(),
            // InternalResourceRecord::CNAME { cname } => todo!(),
            // InternalResourceRecord::SOA { serial, refresh, retry, expire, minimum } => todo!(),
            // InternalResourceRecord::MB {  } => todo!(),
            // InternalResourceRecord::MG {  } => todo!(),
            // InternalResourceRecord::MR {  } => todo!(),
            // InternalResourceRecord::NULL {  } => todo!(),
            // InternalResourceRecord::WKS {  } => todo!(),
            // InternalResourceRecord::PTR { ptrdname } => todo!(),
            InternalResourceRecord::HINFO { cpu, os, ttl: _ } => {
                let mut hinfo_bytes: Vec<u8> = vec![];

                match cpu {
                    Some(value) => {
                        let bytes: Vec<u8> = value.into();
                        hinfo_bytes.extend(bytes);
                    }
                    None => {
                        hinfo_bytes.extend([consts::ARCH.len() as u8]);
                        hinfo_bytes.extend(consts::ARCH.as_bytes());
                    }
                };

                match os {
                    Some(value) => {
                        let bytes: Vec<u8> = value.into();
                        hinfo_bytes.extend(bytes);
                    }
                    None => {
                        hinfo_bytes.extend([consts::OS.len() as u8]);
                        hinfo_bytes.extend(consts::OS.as_bytes());
                    }
                };
                hinfo_bytes
            }
            // InternalResourceRecord::MINFO {  } => todo!(),
            // InternalResourceRecord::MX { preference, exchange } => todo!(),
            // InternalResourceRecord::AXFR {  } => todo!(),
            // InternalResourceRecord::MAILB {  } => todo!(),
            // InternalResourceRecord::MAILA {  } => todo!(),
            // InternalResourceRecord::ALL {  } => todo!(),
            // InternalResourceRecord::InvalidType => todo!(),
            _ => vec![],
        }
    }

    pub fn hexdump(self) {
        utils::hexdump(self.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;
    use std::str::{from_utf8, FromStr};

    use log::debug;

    use crate::enums::RecordType;
    use crate::zones::FileZoneRecord;

    use super::{DNSCharString, InternalResourceRecord};
    #[test]
    fn test_eq_resourcerecord() {
        assert_eq!(
            InternalResourceRecord::A {
                address: 12345,
                ttl: None
            },
            RecordType::A
        );
        assert_eq!(
            InternalResourceRecord::AAAA {
                address: 12345,
                ttl: None
            },
            RecordType::AAAA
        );
    }

    #[test]
    fn test_resourcerecord_from_ipv6_string() {
        // femme::with_level(log::LevelFilter::Debug);
        let fzr = FileZoneRecord {
            name: "test".to_string(),
            rrtype: "AAAA".to_string(),
            rdata: String::from("1234:5678:cafe:beef:ca75:0:4b9:e94d")
                .as_bytes()
                .to_vec(),
            ttl: Some(160),
        };
        debug!("fzr: {:?}", fzr);
        let converted = Ipv6Addr::from_str(&from_utf8(&fzr.rdata).unwrap()).unwrap();
        debug!("conversion: {:?}", converted);
        let rr: InternalResourceRecord = fzr.into();

        debug!("fzr->rr = {:?}", rr);
        assert_eq!(rr, RecordType::AAAA);
        assert_eq!(
            rr.as_bytes(),
            [18, 52, 86, 120, 202, 254, 190, 239, 202, 117, 0, 0, 4, 185, 233, 77].to_vec()
        );
    }
    #[test]
    fn test_dnscharstring() {
        let test: DNSCharString = "hello world".into();
        let testbytes: Vec<u8> = test.into();
        assert_eq!(testbytes[0], 11);
    }

    #[test]
    fn resourcerecord_txt() {
        let foo = InternalResourceRecord::TXT {
            txtdata: DNSCharString::from("Hello world"),
            ttl: None,
        };
        if let InternalResourceRecord::TXT { txtdata, ttl: _ } = foo {
            let foo_bytes: Vec<u8> = txtdata.into();
            assert_eq!(foo_bytes[0], 11);
        };
    }
}

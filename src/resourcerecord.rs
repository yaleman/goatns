use crate::enums::RecordType;
use crate::utils;
use log::*;
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

#[allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum InternalResourceRecord {
    A {
        address: u32,
    }, // 1 a host address
    NS {
        nsdname: DomainName,
    }, // 2 an authoritative name server
    MD {}, // 3 a mail destination (Obsolete - use MX)
    MF {}, // 4 a mail forwarder (Obsolete - use MX)
    CNAME {
        cname: DomainName,
    }, // 5 the canonical name for an alias
    SOA {
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    }, // 6 marks the start of a zone of authority
    MB {}, // 7 a mailbox domain name (EXPERIMENTAL)
    MG {}, // 8 a mail group member (EXPERIMENTAL)
    MR {}, // 9 a mail rename domain name (EXPERIMENTAL)
    NULL {}, // 10 a null RR (EXPERIMENTAL)
    WKS {}, // 11 a well known service description
    PTR {
        ptrdname: DomainName,
    }, // 12 a domain name pointer
    HINFO {}, // 13 host information
    MINFO {}, // 14 mailbox or mail list information
    MX {
        preference: u16,
        exchange: DomainName,
    }, // 15 mail exchange
    TXT {
        txtdata: Vec<u8>,
    }, // 16 text strings
    AAAA {
        address: u128,
    }, // 28 https://www.rfc-editor.org/rfc/rfc3596#section-2.1
    AXFR {}, // 252 A request for a transfer of an entire zone

    MAILB {}, // 253 A request for mailbox-related records (MB, MG or MR)

    MAILA {}, // 254 A request for mail agent RRs (Obsolete - see MX)

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
                InternalResourceRecord::A { address }
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

                InternalResourceRecord::AAAA { address }
            }
            _ => InternalResourceRecord::InvalidType,
        }
    }
}

impl PartialEq<RecordType> for InternalResourceRecord {
    fn eq(&self, other: &RecordType) -> bool {
        match self {
            InternalResourceRecord::A { address: _ } => other == &RecordType::A,
            InternalResourceRecord::AAAA { address: _ } => other == &RecordType::AAAA,
            InternalResourceRecord::ALL {} => other == &RecordType::ALL,
            InternalResourceRecord::AXFR {} => other == &RecordType::AXFR,
            InternalResourceRecord::CNAME { cname: _ } => other == &RecordType::CNAME,
            InternalResourceRecord::HINFO {} => other == &RecordType::HINFO,
            InternalResourceRecord::InvalidType => other == &RecordType::InvalidType,
            InternalResourceRecord::MAILA {} => other == &RecordType::MAILA,
            InternalResourceRecord::MAILB {} => other == &RecordType::MAILB,
            InternalResourceRecord::MB {} => other == &RecordType::MB,
            InternalResourceRecord::MD {} => other == &RecordType::MD,
            InternalResourceRecord::MF {} => other == &RecordType::MF,
            InternalResourceRecord::MG {} => other == &RecordType::MG,
            InternalResourceRecord::MINFO {} => other == &RecordType::MINFO,
            InternalResourceRecord::MR {} => other == &RecordType::MR,
            InternalResourceRecord::MX {
                preference: _,
                exchange: _,
            } => other == &RecordType::MX,
            InternalResourceRecord::NS { nsdname: _ } => other == &RecordType::NS,
            InternalResourceRecord::NULL {} => other == &RecordType::NULL,
            InternalResourceRecord::PTR { ptrdname: _ } => other == &RecordType::PTR,
            InternalResourceRecord::SOA {
                serial: _,
                refresh: _,
                retry: _,
                expire: _,
                minimum: _,
            } => other == &RecordType::SOA,
            InternalResourceRecord::TXT { txtdata: _ } => other == &RecordType::TXT,
            InternalResourceRecord::WKS {} => other == &RecordType::WKS,
        }
    }
}

impl InternalResourceRecord {
    pub fn as_bytes(self: InternalResourceRecord) -> Vec<u8> {
        match self {
            InternalResourceRecord::A { address } => address.to_be_bytes().to_vec(),
            InternalResourceRecord::AAAA { address } => address.to_be_bytes().to_vec(),
            // ResourceRecord::NS { nsdname } => todo!(),
            // ResourceRecord::MD {  } => todo!(),
            // ResourceRecord::MF {  } => todo!(),
            // ResourceRecord::CNAME { cname } => todo!(),
            // ResourceRecord::SOA { serial, refresh, retry, expire, minimum } => todo!(),
            // ResourceRecord::MB {  } => todo!(),
            // ResourceRecord::MG {  } => todo!(),
            // ResourceRecord::MR {  } => todo!(),
            // ResourceRecord::NULL {  } => todo!(),
            // ResourceRecord::WKS {  } => todo!(),
            // ResourceRecord::PTR { ptrdname } => todo!(),
            // ResourceRecord::HINFO {  } => todo!(),
            // ResourceRecord::MINFO {  } => todo!(),
            // ResourceRecord::MX { preference, exchange } => todo!(),
            // ResourceRecord::TXT { txtdata } => todo!(),
            // ResourceRecord::AXFR {  } => todo!(),
            // ResourceRecord::MAILB {  } => todo!(),
            // ResourceRecord::MAILA {  } => todo!(),
            // ResourceRecord::ALL {  } => todo!(),
            // ResourceRecord::InvalidType => todo!(),
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
    use crate::resourcerecord;
    use crate::zones::FileZoneRecord;
    #[test]
    fn test_eq_resourcerecord() {
        assert_eq!(
            resourcerecord::InternalResourceRecord::A { address: 12345 },
            RecordType::A
        );
        assert_eq!(
            resourcerecord::InternalResourceRecord::AAAA { address: 12345 },
            RecordType::AAAA
        );
    }

    #[test]
    fn test_resourcerecord_from_ipv6_string() {
        femme::with_level(log::LevelFilter::Debug);
        let fzr = FileZoneRecord {
            name: "test".to_string(),
            rrtype: "AAAA".to_string(),
            rdata: String::from("1234:5678:cafe:beef:ca75:0:4b9:e94d")
                .as_bytes()
                .to_vec(),
        };
        debug!("fzr: {:?}", fzr);
        let converted = Ipv6Addr::from_str(&from_utf8(&fzr.rdata).unwrap()).unwrap();
        debug!("conversion: {:?}", converted);
        let rr: resourcerecord::InternalResourceRecord = fzr.into();

        debug!("fzr->rr = {:?}", rr);
        assert_eq!(rr, RecordType::AAAA);
        assert_eq!(
            rr.as_bytes(),
            [18, 52, 86, 120, 202, 254, 190, 239, 202, 117, 0, 0, 4, 185, 233, 77].to_vec()
        );
    }
}

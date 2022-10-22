use crate::enums::RecordType;
use crate::utils::{hexdump, name_as_bytes};
use crate::HEADER_BYTES;
use log::*;
use std::env::consts;

use std::str::FromStr;
use std::string::FromUtf8Error;
// use packed_struct::*;

use crate::zones::FileZoneRecord;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainName {
    pub name: String,
}

impl DomainName {
    /// Push the DomainName through the name_as_bytes function
    // TODO:
    pub fn as_bytes(
        &self,
        compress_target: Option<u16>,
        compress_reference: Option<&Vec<u8>>,
    ) -> Vec<u8> {
        name_as_bytes(
            self.name.to_owned().into_bytes(),
            compress_target,
            compress_reference,
        )
    }
}

impl From<&str> for DomainName {
    fn from(input: &str) -> Self {
        let name = match input.contains('@') {
            false => String::from(input),
            true => input.replace('@', "."),
        };
        DomainName { name }
    }
}

impl From<String> for DomainName {
    fn from(name: String) -> Self {
        DomainName { name }
    }
}

impl TryFrom<&Vec<u8>> for DomainName {
    fn try_from(input: &Vec<u8>) -> Result<Self, FromUtf8Error> {
        match String::from_utf8(input.to_owned()) {
            Ok(value) => Ok(DomainName { name: value }),
            Err(error) => Err(error),
        }
    }

    type Error = FromUtf8Error;
}

/// Turn this into the domain-name value
impl From<&DomainName> for Vec<u8> {
    fn from(dn: &DomainName) -> Self {
        name_as_bytes(dn.name.as_bytes().to_vec(), None, None)
    }
}

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

impl DNSCharString {
    /// Returns the bytes for a packet, ie - the length and then the string (truncated to 255 bytes)
    fn as_bytes(&self) -> Vec<u8> {
        let mut res: Vec<u8> = vec![self.data.len() as u8];
        res.extend(&self.data);
        // <character-string> is a single length octet followed by that number of characters.  <character-string> is treated as binary information, and can be up to 256 characters in length (including the length octet).
        res.truncate(257);

        // TODO: I wonder if we can automagically split this and return it as individual records? it'd have to happen up the stack.
        res
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum InternalResourceRecord {
    A {
        address: u32,
        ttl: u32,
    }, // 1 a host address
    NAPTR {
        ttl: u32,
        ///     Domain - The domain name to which this resource record refers.  This is the 'key' for this entry in the rule database.  This value will either be the first well known key (<something>.uri.arpa for example) or a new key that is the output of a replacement or regexp rewrite. Beyond this, it has the standard DNS requirements.
        domain: DomainName,
        // A 16-bit unsigned integer specifying the order in which the NAPTR records MUST be processed to ensure the correct ordering of rules.  Low numbers are processed before high numbers, and once a NAPTR is found whose rule "matches" the target, the client MUST NOT consider any NAPTRs with a higher value for order (except as noted below for the Flags field).
        order: u16,

        /* A 16-bit unsigned integer that specifies the order in which NAPTR
        records with equal "order" values SHOULD be processed, low
        numbers being processed before high numbers.  This is similar to
        the preference field in an MX record, and is used so domain
        administrators can direct clients towards more capable hosts or
        lighter weight protocols.  A client MAY look at records with
        higher preference values if it has a good reason to do so such as
        not understanding the preferred protocol or service.

        The important difference between Order and Preference is that
        once a match is found the client MUST NOT consider records with a
        different Order but they MAY process records with the same Order
        but different Preferences.  I.e., Preference is used to give weight
        to rules that are considered the same from an authority
        standpoint but not from a simple load balancing standpoint.*/
        preference: u16,

        // A <character-string> containing flags to control aspects of the
        // rewriting and interpretation of the fields in the record.  Flags
        // are single characters from the set [A-Z0-9].  The case of the
        // alphabetic characters is not significant.

        // At this time only four flags, "S", "A", "U", and "P", are
        // defined.  The "S", "A" and "U" flags denote a terminal lookup.
        // This means that this NAPTR record is the last one and that the
        // flag determines what the next stage should be.  The "S" flag
        // means that the next lookup should be for SRV records [4].  See
        // Section 5 for additional information on how NAPTR uses the SRV
        // record type.  "A" means that the next lookup should be for either
        // an A, AAAA, or A6 record.  The "U" flag means that the next step
        // is not a DNS lookup but that the output of the Regexp field is an
        // URI that adheres to the 'absoluteURI' production found in the
        // ABNF of RFC 2396 [9].  Since there may be applications that use
        // NAPTR to also lookup aspects of URIs, implementors should be
        // aware that this may cause loop conditions and should act
        // accordingly.
        flags: String,
    },
    NS {
        nsdname: DomainName,
        ttl: u32,
    }, // 2 an authoritative name server
    MD {
        ttl: u32,
    }, // 3 a mail destination (Obsolete - use MX)
    MF {
        ttl: u32,
    }, // 4 a mail forwarder (Obsolete - use MX)
    CNAME {
        cname: DomainName,
        ttl: u32,
    }, // 5 the canonical name for an alias
    SOA {
        // The zone that this SOA record is for - eg hello.goat or example.com
        zone: DomainName,
        /// The <domain-name> of the name server that was the original or primary source of data for this zone.
        mname: DomainName,
        /// A <domain-name> which specifies the mailbox of the person responsible for this zone. eg: `dns.example.com` is actually `dns@example.com`
        rname: DomainName,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
        // this doesn't get a TTL, since that's an expire?
    }, // 6 marks the start of a zone of authority
    MB {
        ttl: u32,
    }, // 7 a mailbox domain name (EXPERIMENTAL)
    MG {
        ttl: u32,
    }, // 8 a mail group member (EXPERIMENTAL)
    MR {
        ttl: u32,
    }, // 9 a mail rename domain name (EXPERIMENTAL)
    NULL {
        ttl: u32,
    }, // 10 a null RR (EXPERIMENTAL)
    WKS {
        ttl: u32,
    }, // 11 a well known service description
    PTR {
        ptrdname: DomainName,
        ttl: u32,
    }, // 12 a domain name pointer
    HINFO {
        cpu: Option<DNSCharString>,
        os: Option<DNSCharString>,
        ttl: u32,
    }, // 13 host information
    MINFO {
        ttl: u32,
    }, // 14 mailbox or mail list information
    MX {
        preference: u16,
        exchange: DomainName,
        ttl: u32,
    }, // 15 mail exchange
    TXT {
        txtdata: DNSCharString,
        ttl: u32,
    }, // 16 text strings
    AAAA {
        address: u128,
        ttl: u32,
    }, // 28 https://www.rfc-editor.org/rfc/rfc3596#section-2.1
    AXFR {
        ttl: u32,
    }, // 252 A request for a transfer of an entire zone

    MAILB {
        ttl: u32,
    }, // 253 A request for mailbox-related records (MB, MG or MR)

    // MAILA {
    //     ttl: u32,
    // }, // 254 A request for mail agent RRs (Obsolete - see MX)
    ALL {}, // 255 A request for all records (*)
    InvalidType,
}

impl TryFrom<FileZoneRecord> for InternalResourceRecord {
    // TODO: This should be a try_into because we're parsing text
    /// This is where we convert from the JSON blob in the file to an internal representation of the data.
    fn try_from(record: FileZoneRecord) -> Result<Self, String> {
        match record.rrtype.as_str() {
            "A" => {
                // let address = match from_utf8(&record.rdata) {
                //     Ok(value) => value,
                //     Err(error) => {
                //         error!(
                //             "Failed to parse {:?} to string in A record: {:?}",
                //             record.rdata, error
                //         );
                //         return InternalResourceRecord::InvalidType;
                //     }
                // };
                let address: u32 = match std::net::Ipv4Addr::from_str(&record.rdata) {
                    Ok(value) => value.into(),
                    Err(error) => {
                        error!(
                            "Failed to parse {:?} into an IPv4 address: {:?}",
                            record.rdata, error
                        );
                        0u32
                    }
                };
                Ok(InternalResourceRecord::A {
                    address,
                    ttl: record.ttl,
                })
            }
            "AAAA" => {
                let address: u128 = match std::net::Ipv6Addr::from_str(&record.rdata) {
                    Ok(value) => {
                        let res: u128 = value.into();
                        trace!("Encoding {:?} as {:?}", value, res);
                        res
                    }
                    Err(error) => {
                        return Err(format!(
                            "Failed to parse {:?} into an IPv6 address: {:?}",
                            record.rdata, error
                        ));
                    }
                };

                Ok(InternalResourceRecord::AAAA {
                    address,
                    ttl: record.ttl,
                })
            }
            "TXT" => Ok(InternalResourceRecord::TXT {
                txtdata: DNSCharString {
                    data: record.rdata.into_bytes(),
                },
                ttl: record.ttl,
            }),
            "PTR" => Ok(InternalResourceRecord::PTR {
                ptrdname: DomainName::from(record.rdata),
                ttl: record.ttl,
            }),
            "NS" => Ok(InternalResourceRecord::NS {
                nsdname: DomainName::from(record.rdata),
                ttl: record.ttl,
            }),
            "MX" => {
                let split_bit: Vec<&str> = record.rdata.split(' ').collect();
                if split_bit.len() != 2 {
                    return Err(format!(
                        "While trying to parse MX record, got '{:?}' which is wrong.",
                        split_bit
                    ));
                };
                let pref = match u16::from_str(split_bit[0]) {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(format!(
                            "Failed to parse {} into number: {:?}",
                            split_bit[0], error
                        ))
                    }
                };
                trace!("got pref {}, now {pref}", split_bit[0]);
                Ok(InternalResourceRecord::MX {
                    preference: pref,
                    exchange: DomainName::from(split_bit[1]),
                    ttl: record.ttl,
                })
            }
            _ => Err("Invalid type specified!".to_string()),
        }
    }

    type Error = String;
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
            // InternalResourceRecord::MAILA { ttl: _ } => other == &RecordType::MAILA,
            InternalResourceRecord::MAILB { ttl: _ } => other == &RecordType::MAILB,
            InternalResourceRecord::NAPTR {
                flags: _,
                preference: _,
                domain: _,
                order: _,
                ttl: _,
            } => other == &RecordType::NAPTR,
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
                zone: _,
                mname: _,
                rname: _,
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

// impl From<&InternalResourceRecord> for Vec<u8> {
//     fn from(record: &InternalResourceRecord) -> Self {
//         record.to_owned().as_bytes()
//     }
// }

impl InternalResourceRecord {
    pub fn as_bytes(self: &InternalResourceRecord, question: &Vec<u8>) -> Vec<u8> {
        match self {
            InternalResourceRecord::A { address, ttl: _ } => address.to_be_bytes().to_vec(),
            InternalResourceRecord::AAAA { address, ttl: _ } => address.to_be_bytes().to_vec(),
            InternalResourceRecord::TXT { txtdata, ttl: _ } => {
                txtdata.as_bytes()
            }

            // InternalResourceRecord::MD {  } => todo!(),
            // InternalResourceRecord::MF {  } => todo!(),
            // InternalResourceRecord::CNAME { cname } => todo!(),
            InternalResourceRecord::SOA {
                zone,
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => {
                // TODO: the name_as_bytes needs to be able to take a source DomainName to work out the bytes compression stuff
                let zone_as_bytes = zone.name.as_bytes().to_vec();
                let mut res: Vec<u8> =
                    mname.as_bytes(Some(HEADER_BYTES as u16), Some(&zone_as_bytes));
                // TODO: the name_as_bytes needs to be able to take a source DomainName to work out the bytes compression stuff
                res.extend(rname.as_bytes(Some(HEADER_BYTES as u16), Some(&zone_as_bytes)));
                res.extend(serial.to_be_bytes());
                res.extend(refresh.to_be_bytes());
                res.extend(retry.to_be_bytes());
                res.extend(expire.to_be_bytes());
                res.extend(minimum.to_be_bytes());
                res
            }
            // InternalResourceRecord::MB {  } => todo!(),
            // InternalResourceRecord::MG {  } => todo!(),
            // InternalResourceRecord::MR {  } => todo!(),
            // InternalResourceRecord::NULL {  } => todo!(),
            // InternalResourceRecord::WKS {  } => todo!(),
            InternalResourceRecord::NS { nsdname, ttl: _ } => {
                nsdname.as_bytes(Some(HEADER_BYTES as u16), Some(question))
            }
            InternalResourceRecord::PTR { ptrdname, ttl: _ } => {
                ptrdname.as_bytes(Some(HEADER_BYTES as u16), Some(question))
            }
            InternalResourceRecord::HINFO { cpu, os, ttl: _ } => {
                let mut hinfo_bytes: Vec<u8> = vec![];
                match cpu {
                    Some(value) => {
                        hinfo_bytes.extend(&value.as_bytes());
                    }
                    None => {
                        hinfo_bytes.extend([consts::ARCH.len() as u8]);
                        hinfo_bytes.extend(consts::ARCH.as_bytes());
                    }
                };

                match os {
                    Some(value) => {
                        hinfo_bytes.extend(&value.as_bytes());
                    }
                    None => {
                        hinfo_bytes.extend([consts::OS.len() as u8]);
                        hinfo_bytes.extend(consts::OS.as_bytes());
                    }
                };
                hinfo_bytes
            }
            // InternalResourceRecord::MINFO {  } => todo!(),
            InternalResourceRecord::MX { preference, exchange , ttl: _} => {
                let mut mx_bytes: Vec<u8> = preference.to_be_bytes().into();
                mx_bytes.extend(exchange.as_bytes(Some(HEADER_BYTES as u16), Some(question)));
                mx_bytes
            }
            ,
            // InternalResourceRecord::AXFR {  } => todo!(),
            // InternalResourceRecord::MAILB {  } => todo!(),
            // InternalResourceRecord::MAILA {  } => todo!(),
            // InternalResourceRecord::ALL {  } => todo!(),
            // InternalResourceRecord::InvalidType => todo!(),
            _ => unimplemented!(),
        }
    }

    pub fn hexdump(self) {
        hexdump(self.as_bytes(&vec![]));
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;
    use std::str::FromStr;

    use log::debug;

    use crate::enums::RecordType;
    use crate::zones::FileZoneRecord;

    use super::{DNSCharString, InternalResourceRecord};
    #[test]
    fn test_eq_resourcerecord() {
        assert_eq!(
            InternalResourceRecord::A {
                address: 12345,
                ttl: 1
            },
            RecordType::A
        );
        assert_eq!(
            InternalResourceRecord::AAAA {
                address: 12345,
                ttl: 1
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
            rdata: String::from("1234:5678:cafe:beef:ca75:0:4b9:e94d"),
            ttl: 160u32,
        };
        debug!("fzr: {fzr}");
        let converted = Ipv6Addr::from_str(&fzr.rdata).unwrap();
        debug!("conversion: {:?}", converted);
        let rr: InternalResourceRecord = match fzr.try_into() {
            Ok(value) => value,
            Err(error) => panic!("Failed to get resource record: {:?}", error),
        };

        debug!("fzr->rr = {rr:?}");
        assert_eq!(rr, RecordType::AAAA);
        assert_eq!(
            rr.as_bytes(&vec![]),
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
            ttl: 1,
        };
        if let InternalResourceRecord::TXT { txtdata, ttl: _ } = foo {
            let foo_bytes: Vec<u8> = txtdata.into();
            assert_eq!(foo_bytes[0], 11);
        };
    }
}

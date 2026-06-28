use crate::HEADER_BYTES;
use crate::db::entities;
use crate::enums::{RecordClass, RecordType};
use crate::error::GoatNsError;
use crate::utils::{dms_to_u32, hexdump, name_as_bytes};
use core::fmt::Debug;
use goat_lib::constants::{DEFAULT_LOC_HORIZ_PRE, DEFAULT_LOC_SIZE, DEFAULT_LOC_VERT_PRE};
use goat_lib::validators::{CAA_TAG_VALIDATOR, URI_RECORD};
use num_traits::Num;
use packed_struct::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize, Serializer};
use std::env::consts;
use std::str::{FromStr, from_utf8};
use std::string::FromUtf8Error;
use tracing::{error, instrument, trace, warn};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DomainName {
    pub name: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum NameAsBytes {
    Compressed(Vec<u8>),
    Uncompressed(Vec<u8>),
}

impl PartialEq<Vec<u8>> for NameAsBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        match self {
            NameAsBytes::Compressed(value) => value == other,
            NameAsBytes::Uncompressed(value) => value == other,
        }
    }
}

impl NameAsBytes {
    /// Sometimes you want the trailing null to be added!
    pub fn as_bytes_with_trailing_null(&self) -> Vec<u8> {
        match self {
            NameAsBytes::Compressed(value) => value.to_vec(),
            NameAsBytes::Uncompressed(value) => value.iter().copied().chain(vec![0]).collect(),
        }
    }
}

impl From<NameAsBytes> for Vec<u8> {
    fn from(input: NameAsBytes) -> Self {
        match input {
            NameAsBytes::Compressed(value) => value,
            NameAsBytes::Uncompressed(value) => value,
        }
    }
}

impl DomainName {
    /// Push the DomainName through the name_as_bytes function
    #[instrument(level = "debug")]
    pub(crate) fn as_bytes_compressed(
        &self,
        compress_target: Option<u16>,
        compress_reference: Option<&Vec<u8>>,
    ) -> Result<NameAsBytes, GoatNsError> {
        name_as_bytes(
            &self.name.to_owned().into_bytes(),
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
impl TryFrom<&DomainName> for Vec<u8> {
    type Error = GoatNsError;
    fn try_from(dn: &DomainName) -> Result<Self, Self::Error> {
        Ok(name_as_bytes(dn.name.as_bytes(), None, None)?.into())
    }
}

#[derive(Debug, PackedStruct, PartialEq, Eq, Clone)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "16")]
pub struct LocRecord {
    #[packed_field(bits = "0..8", endian = "msb")]
    pub version: u8,
    // #[packed_field(bits = "9..13", endian = "msb")]
    // pub size_higher: u8,
    // #[packed_field(bits = "13..16", endian = "msb")]
    // pub size_lower: u8,
    #[packed_field(bits = "9..16", endian = "msb")]
    pub size: u8,
    #[packed_field(bits = "16..24", endian = "msb")]
    pub horiz_pre: u8,
    #[packed_field(bits = "24..32", endian = "msb")]
    pub vert_pre: u8,
    #[packed_field(bits = "32..64", endian = "msb")]
    pub latitude: u32,
    #[packed_field(bits = "64..96", endian = "msb")]
    pub longitude: u32,
    #[packed_field(bits = "96..128", endian = "msb")]
    pub altitude: i32,
}

/// `<character-string>` is a single length octet followed by that number of characters.  `<character-string>` is treated as binary information, and can be up to 256 characters in length (including the length octet).
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct DNSCharString {
    pub data: Vec<u8>,
}

impl Serialize for DNSCharString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[allow(clippy::expect_used)]
        let res = from_utf8(&self.data).expect("This shouldn't ever fail");
        serializer.serialize_str(res)
    }
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
    /// Returns the bytes for a packet, ie - the length and then the string
    fn as_bytes(&self) -> Vec<u8> {
        let mut res: Vec<u8> = self.data.to_vec();
        // <character-string> is a single length octet followed by that number of characters.  <character-string> is treated as binary information, and can be up to 256 characters in length (including the length octet).
        res.truncate(255);
        res.insert(0, res.len() as u8);
        res
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
/// Internal representation of a resource record
pub enum InternalResourceRecord {
    /// A single host address
    A {
        #[serde(serialize_with = "crate::serializers::a_to_ip")]
        address: u32,
        ttl: u32,
        rclass: RecordClass,
    },
    AAAA {
        #[serde(serialize_with = "crate::serializers::aaaa_to_ip")]
        address: u128,
        ttl: u32,
        rclass: RecordClass,
    }, // 28 https://www.rfc-editor.org/rfc/rfc3596#section-2.1
    AXFR {
        ttl: u32,
        rclass: RecordClass,
    }, // 252 A request for a transfer of an entire zone
    // [RFC8659](https://www.rfc-editor.org/rfc/rfc8659) - CAA Record
    CAA {
        flag: u8,
        /// Tags MAY contain ASCII characters "a" through "z", "A" through "Z", and the numbers 0 through 9. Tags MUST NOT contain any other characters. Matching of tags is case insensitive.
        tag: DNSCharString,
        /// A sequence of octets representing the Property Value. Property Values are encoded as binary values and MAY employ sub‑formats.
        value: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    CNAME {
        cname: DomainName,
        ttl: u32,
        rclass: RecordClass,
    }, // 5 the canonical name for an alias
    LOC {
        ttl: u32,
        rclass: RecordClass,
        /// Version number of the representation.  This must be zero. Implementations are required to check this field and make no assumptions about the format of unrecognized versions.
        version: u8,
        size: u8,
        horiz_pre: u8,
        vert_pre: u8,
        latitude: u32,
        longitude: u32,
        altitude: i32,
    },
    NAPTR {
        ttl: u32,
        rclass: RecordClass,
        ///     Domain - The domain name to which this resource record refers.  This is the 'key' for this entry in the rule database.  This value will either be the first well known key (`<something>.uri.arpa` for example) or a new key that is the output of a replacement or regexp rewrite. Beyond this, it has the standard DNS requirements.
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
        // NAPTR to also lookup aspects of URIs, implementers should be
        // aware that this may cause loop conditions and should act
        // accordingly.
        flags: String,
    },
    NS {
        nsdname: DomainName,
        ttl: u32,
        rclass: RecordClass,
    }, // 2 an authoritative name server
    SOA {
        // The zone that this SOA record is for - eg hello.goat or example.com
        zone: DomainName,
        /// The `domain-name` of the name server that was the original or primary source of data for this zone.
        mname: DomainName,
        /// A `domain-name` which specifies the mailbox of the person responsible for this zone. eg: `dns.example.com` is actually `dns@example.com`
        rname: DomainName,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
        rclass: RecordClass,
    }, // 6 marks the start of a zone of authority

    PTR {
        ptrdname: DomainName,
        ttl: u32,
        rclass: RecordClass,
    }, // 12 a domain name pointer
    /// RFC1035
    HINFO {
        cpu: Option<DNSCharString>,
        os: Option<DNSCharString>,
        ttl: u32,
        rclass: RecordClass,
    }, // 13 host information
    MX {
        preference: u16,
        exchange: DomainName,
        ttl: u32,
        rclass: RecordClass,
    }, // 15 mail exchange
    TXT {
        txtdata: DNSCharString,
        ttl: u32,
        class: RecordClass,
    }, // 16 text strings
    URI {
        priority: u16,
        weight: u16,
        target: DNSCharString,
        ttl: u32,
        rclass: RecordClass,
    },
    DNSKEY {
        flags: u16,
        protocol: u8,
        algorithm: u8,
        public_key: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    RRSIG {
        type_covered: RecordType,
        algorithm: u8,
        labels: u8,
        original_ttl: u32,
        signature_expiration: u32,
        signature_inception: u32,
        key_tag: u16,
        signer_name: DomainName,
        signature: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    NSEC {
        next_domain_name: DomainName,
        type_bit_maps: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    NSEC3 {
        hash_algorithm: u8,
        flags: u8,
        iterations: u16,
        salt: Vec<u8>,
        next_hashed_owner_name: Vec<u8>,
        type_bit_maps: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    NSEC3PARAM {
        hash_algorithm: u8,
        flags: u8,
        iterations: u16,
        salt: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    DS {
        key_tag: u16,
        algorithm: u8,
        digest_type: u8,
        digest: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    CDNSKEY {
        flags: u16,
        protocol: u8,
        algorithm: u8,
        public_key: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    CDS {
        key_tag: u16,
        algorithm: u8,
        digest_type: u8,
        digest: Vec<u8>,
        ttl: u32,
        rclass: RecordClass,
    },
    InvalidType,
}

fn parse_domain_name_from_wire(input: &[u8]) -> Result<(DomainName, Vec<u8>), GoatNsError> {
    let mut labels: Vec<&str> = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let label_len = input[pos] as usize;
        if label_len == 0 {
            pos += 1;
            break;
        }
        if pos + 1 + label_len > input.len() {
            return Err(GoatNsError::Generic(
                "domain name label extends past end of rdata".to_string(),
            ));
        }
        let label = from_utf8(&input[pos + 1..pos + 1 + label_len]).map_err(|e| {
            GoatNsError::Generic(format!("invalid utf8 in domain name label: {e:?}"))
        })?;
        labels.push(label);
        pos += 1 + label_len;
    }
    let name = if labels.is_empty() {
        ".".to_string()
    } else {
        labels.join(".")
    };
    Ok((DomainName::from(name), input[pos..].to_vec()))
}

impl InternalResourceRecord {
    pub fn is_type(&self, rrtype: RecordType) -> bool {
        match self {
            InternalResourceRecord::A { .. } => rrtype == RecordType::A,
            InternalResourceRecord::AAAA { .. } => rrtype == RecordType::AAAA,
            InternalResourceRecord::AXFR { .. } => rrtype == RecordType::AXFR,
            InternalResourceRecord::CAA { .. } => rrtype == RecordType::CAA,
            InternalResourceRecord::CDNSKEY { .. } => rrtype == RecordType::CDNSKEY,
            InternalResourceRecord::CDS { .. } => rrtype == RecordType::CDS,
            InternalResourceRecord::CNAME { .. } => rrtype == RecordType::CNAME,
            InternalResourceRecord::DNSKEY { .. } => rrtype == RecordType::DNSKEY,
            InternalResourceRecord::DS { .. } => rrtype == RecordType::DS,
            InternalResourceRecord::HINFO { .. } => rrtype == RecordType::HINFO,
            InternalResourceRecord::InvalidType => rrtype == RecordType::InvalidType,
            InternalResourceRecord::LOC { .. } => rrtype == RecordType::LOC,
            InternalResourceRecord::MX { .. } => rrtype == RecordType::MX,
            InternalResourceRecord::NAPTR { .. } => rrtype == RecordType::NAPTR,
            InternalResourceRecord::NSEC { .. } => rrtype == RecordType::NSEC,
            InternalResourceRecord::NSEC3 { .. } => rrtype == RecordType::NSEC3,
            InternalResourceRecord::NSEC3PARAM { .. } => rrtype == RecordType::NSEC3PARAM,
            InternalResourceRecord::NS { .. } => rrtype == RecordType::NS,
            InternalResourceRecord::PTR { .. } => rrtype == RecordType::PTR,
            InternalResourceRecord::RRSIG { .. } => rrtype == RecordType::RRSIG,
            InternalResourceRecord::SOA { .. } => rrtype == RecordType::SOA,
            InternalResourceRecord::TXT { .. } => rrtype == RecordType::TXT,
            InternalResourceRecord::URI { .. } => rrtype == RecordType::URI,
        }
    }
}

impl InternalResourceRecord {
    pub fn new(
        name: &str,
        rrtype: u16,
        rdata: &str,
        ttl: u32,
        rclass: u16,
    ) -> Result<Self, GoatNsError> {
        let rclass = RecordClass::from(&rclass);
        match RecordType::from(&rrtype) {
            RecordType::A => {
                let address: u32 = match std::net::Ipv4Addr::from_str(rdata) {
                    Ok(value) => value.into(),
                    Err(error) => {
                        error!("Failed to parse {rdata:?} into an IPv4 address: {error:?}");
                        0u32
                    }
                };
                Ok(InternalResourceRecord::A {
                    address,
                    ttl,
                    rclass,
                })
            }
            RecordType::AAAA => {
                let address: u128 = match std::net::Ipv6Addr::from_str(rdata) {
                    Ok(value) => {
                        let res: u128 = value.into();
                        trace!("Encoding {value:?} as {res:?}");
                        res
                    }
                    Err(error) => {
                        return Err(GoatNsError::Generic(format!(
                            "Failed to parse {rdata:?} into an IPv6 address: {error:?}"
                        )));
                    }
                };
                Ok(InternalResourceRecord::AAAA {
                    address,
                    ttl,
                    rclass,
                })
            }
            RecordType::CNAME => Ok(InternalResourceRecord::CNAME {
                cname: DomainName::from(rdata),
                ttl,
                rclass,
            }),
            RecordType::PTR => Ok(InternalResourceRecord::PTR {
                ptrdname: DomainName::from(rdata),
                ttl,
                rclass,
            }),
            RecordType::TXT => Ok(InternalResourceRecord::TXT {
                txtdata: DNSCharString {
                    data: rdata.as_bytes().to_vec(),
                },
                ttl,
                class: rclass,
            }),
            RecordType::MX => {
                let split_bit: Vec<&str> = rdata.split(' ').collect();
                if split_bit.len() != 2 {
                    return Err(GoatNsError::Generic(format!(
                        "While trying to parse MX record, got '{split_bit:?}' which is wrong."
                    )));
                };
                let pref = match u16::from_str(split_bit[0]) {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(GoatNsError::Generic(format!(
                            "Failed to parse {} into number: {error:?}",
                            split_bit[0]
                        )));
                    }
                };
                trace!("got pref {}, now {pref}", split_bit[0]);
                Ok(InternalResourceRecord::MX {
                    preference: pref,
                    exchange: DomainName::from(split_bit[1]),
                    ttl,
                    rclass,
                })
            }
            RecordType::NS => Ok(InternalResourceRecord::NS {
                nsdname: DomainName::from(rdata),
                ttl,
                rclass,
            }),
            RecordType::CAA => {
                let split_bit: Vec<&str> = rdata.split(' ').collect();
                if split_bit.len() < 3 {
                    return Err(GoatNsError::Generic(format!(
                        "While trying to parse CAA record, got '{split_bit:?}' which is wrong."
                    )));
                };
                let flag = match u8::from_str(split_bit[0]) {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(GoatNsError::Generic(format!(
                            "Failed to parse {} into number: {error:?}",
                            split_bit[0]
                        )));
                    }
                };
                let tag = DNSCharString::from(split_bit[1]);
                if !CAA_TAG_VALIDATOR.is_match(split_bit[1]) {
                    return Err(GoatNsError::Generic(format!(
                        "Invalid tag value {:?} for {}",
                        split_bit[1], name
                    )));
                };
                let value = split_bit[2..].to_vec().join(" ").as_bytes().to_vec();
                Ok(InternalResourceRecord::CAA {
                    flag,
                    tag,
                    value,
                    ttl,
                    rclass,
                })
            }
            RecordType::LOC => {
                let res: FileLocRecord = FileLocRecord::try_from(rdata)?;
                Ok(InternalResourceRecord::LOC {
                    ttl,
                    rclass,
                    version: 0,
                    size: res.size,
                    horiz_pre: res.horiz_pre,
                    vert_pre: res.vert_pre,
                    latitude: dms_to_u32(res.d1, res.m1, res.s1, res.lat_dir == *"N"),
                    longitude: dms_to_u32(res.d2, res.m2, res.s2, res.lon_dir == *"E"),
                    altitude: res.alt,
                })
            }
            RecordType::URI => {
                let matches = match URI_RECORD.captures(rdata) {
                    Some(value) => value,
                    None => {
                        return Err(GoatNsError::Generic(
                            "Failed to parse URL record!".to_string(),
                        ));
                    }
                };
                let priority = match matches.name("priority") {
                    Some(value) => match value.as_str().parse::<u16>() {
                        Ok(value) => value,
                        Err(err) => {
                            return Err(GoatNsError::Generic(format!(
                                "Failed to parse priority into u16: {err:?}"
                            )));
                        }
                    },
                    None => {
                        return Err(GoatNsError::Generic(
                            "No target found in record?".to_string(),
                        ));
                    }
                };
                let weight = match matches.name("weight") {
                    Some(value) => match value.as_str().parse::<u16>() {
                        Ok(value) => value,
                        Err(err) => {
                            return Err(GoatNsError::Generic(format!(
                                "Failed to parse weight into u16: {err:?}"
                            )));
                        }
                    },
                    None => {
                        return Err(GoatNsError::Generic(
                            "No target found in record?".to_string(),
                        ));
                    }
                };
                let target = match matches.name("target") {
                    Some(value) => DNSCharString::from(value.as_str()),
                    None => {
                        return Err(GoatNsError::Generic(
                            "No target found in record?".to_string(),
                        ));
                    }
                };
                Ok(InternalResourceRecord::URI {
                    priority,
                    weight,
                    target,
                    ttl,
                    rclass,
                })
            }
            RecordType::DNSKEY => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode DNSKEY rdata: {e:?}"))
                })?;
                if bytes.len() < 4 {
                    return Err(GoatNsError::Generic(
                        "DNSKEY rdata too short (need >= 4 bytes)".to_string(),
                    ));
                }
                let flags = u16::from_be_bytes([bytes[0], bytes[1]]);
                let protocol = bytes[2];
                let algorithm = bytes[3];
                let public_key = bytes[4..].to_vec();
                Ok(InternalResourceRecord::DNSKEY {
                    flags,
                    protocol,
                    algorithm,
                    public_key,
                    ttl,
                    rclass,
                })
            }
            RecordType::DS => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode DS rdata: {e:?}"))
                })?;
                if bytes.len() < 4 {
                    return Err(GoatNsError::Generic(
                        "DS rdata too short (need >= 4 bytes)".to_string(),
                    ));
                }
                let key_tag = u16::from_be_bytes([bytes[0], bytes[1]]);
                let algorithm = bytes[2];
                let digest_type = bytes[3];
                let digest = bytes[4..].to_vec();
                Ok(InternalResourceRecord::DS {
                    key_tag,
                    algorithm,
                    digest_type,
                    digest,
                    ttl,
                    rclass,
                })
            }
            RecordType::CDNSKEY => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode CDNSKEY rdata: {e:?}"))
                })?;
                if bytes.len() < 4 {
                    return Err(GoatNsError::Generic(
                        "CDNSKEY rdata too short (need >= 4 bytes)".to_string(),
                    ));
                }
                let flags = u16::from_be_bytes([bytes[0], bytes[1]]);
                let protocol = bytes[2];
                let algorithm = bytes[3];
                let public_key = bytes[4..].to_vec();
                Ok(InternalResourceRecord::CDNSKEY {
                    flags,
                    protocol,
                    algorithm,
                    public_key,
                    ttl,
                    rclass,
                })
            }
            RecordType::CDS => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode CDS rdata: {e:?}"))
                })?;
                if bytes.len() < 4 {
                    return Err(GoatNsError::Generic(
                        "CDS rdata too short (need >= 4 bytes)".to_string(),
                    ));
                }
                let key_tag = u16::from_be_bytes([bytes[0], bytes[1]]);
                let algorithm = bytes[2];
                let digest_type = bytes[3];
                let digest = bytes[4..].to_vec();
                Ok(InternalResourceRecord::CDS {
                    key_tag,
                    algorithm,
                    digest_type,
                    digest,
                    ttl,
                    rclass,
                })
            }
            RecordType::RRSIG => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode RRSIG rdata: {e:?}"))
                })?;
                if bytes.len() < 18 {
                    return Err(GoatNsError::Generic(
                        "RRSIG rdata too short (need >= 18 bytes before signer name)".to_string(),
                    ));
                }
                let type_covered = RecordType::from(&u16::from_be_bytes([bytes[0], bytes[1]]));
                let algorithm = bytes[2];
                let labels = bytes[3];
                let original_ttl = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                let signature_expiration =
                    u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                let signature_inception =
                    u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
                let key_tag = u16::from_be_bytes([bytes[16], bytes[17]]);
                let (signer_name, rest) = parse_domain_name_from_wire(&bytes[18..])?;
                let signature = rest.to_vec();
                Ok(InternalResourceRecord::RRSIG {
                    type_covered,
                    algorithm,
                    labels,
                    original_ttl,
                    signature_expiration,
                    signature_inception,
                    key_tag,
                    signer_name,
                    signature,
                    ttl,
                    rclass,
                })
            }
            RecordType::NSEC => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode NSEC rdata: {e:?}"))
                })?;
                let (next_domain_name, rest) = parse_domain_name_from_wire(&bytes)?;
                let type_bit_maps = rest.to_vec();
                Ok(InternalResourceRecord::NSEC {
                    next_domain_name,
                    type_bit_maps,
                    ttl,
                    rclass,
                })
            }
            RecordType::NSEC3 => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode NSEC3 rdata: {e:?}"))
                })?;
                if bytes.len() < 5 {
                    return Err(GoatNsError::Generic("NSEC3 rdata too short".to_string()));
                }
                let hash_algorithm = bytes[0];
                let flags = bytes[1];
                let iterations = u16::from_be_bytes([bytes[2], bytes[3]]);
                let salt_len = bytes[4] as usize;
                if bytes.len() < 5 + salt_len + 1 {
                    return Err(GoatNsError::Generic(
                        "NSEC3 rdata too short for salt".to_string(),
                    ));
                }
                let salt = bytes[5..5 + salt_len].to_vec();
                let hash_len = bytes[5 + salt_len] as usize;
                if bytes.len() < 5 + salt_len + 1 + hash_len {
                    return Err(GoatNsError::Generic(
                        "NSEC3 rdata too short for hash".to_string(),
                    ));
                }
                let next_hashed_owner_name =
                    bytes[5 + salt_len + 1..5 + salt_len + 1 + hash_len].to_vec();
                let type_bit_maps = bytes[5 + salt_len + 1 + hash_len..].to_vec();
                Ok(InternalResourceRecord::NSEC3 {
                    hash_algorithm,
                    flags,
                    iterations,
                    salt,
                    next_hashed_owner_name,
                    type_bit_maps,
                    ttl,
                    rclass,
                })
            }
            RecordType::NSEC3PARAM => {
                let bytes = hex::decode(rdata).map_err(|e| {
                    GoatNsError::Generic(format!("Failed to hex-decode NSEC3PARAM rdata: {e:?}"))
                })?;
                if bytes.len() < 5 {
                    return Err(GoatNsError::Generic(
                        "NSEC3PARAM rdata too short".to_string(),
                    ));
                }
                let hash_algorithm = bytes[0];
                let flags = bytes[1];
                let iterations = u16::from_be_bytes([bytes[2], bytes[3]]);
                let salt_len = bytes[4] as usize;
                let salt = bytes[5..5 + salt_len].to_vec();
                Ok(InternalResourceRecord::NSEC3PARAM {
                    hash_algorithm,
                    flags,
                    iterations,
                    salt,
                    ttl,
                    rclass,
                })
            }
            _ => Err(GoatNsError::Generic("Invalid type specified!".to_string())),
        }
    }
}

impl TryFrom<entities::records::Model> for InternalResourceRecord {
    type Error = GoatNsError;
    fn try_from(record: entities::records::Model) -> Result<Self, Self::Error> {
        if check_long_labels(&record.name) {
            return Err(GoatNsError::Generic(format!(
                "At least one label is of length over 63 in name {}! I'm refusing to serve this record.",
                record.name
            )));
        };

        let Some(ttl) = record.ttl else {
            return Err(GoatNsError::Generic(format!(
                "Record {} has no TTL set! I'm refusing to serve this record.",
                record.name
            )));
        };

        if record.name.len() > 255 {
            return Err(GoatNsError::Generic(format!(
                "The length of name ({}) is over 255 octets! ({}) I'm refusing to serve this record.",
                record.name,
                record.name.len()
            )));
        };

        Self::new(
            &record.name,
            record.rrtype,
            &record.rdata,
            ttl,
            record.rclass,
        )
    }
}

impl TryFrom<entities::records_merged::Model> for InternalResourceRecord {
    type Error = GoatNsError;
    /// This is where we convert from the JSON blob in the file to an internal representation of the data.
    fn try_from(record: entities::records_merged::Model) -> Result<Self, Self::Error> {
        if check_long_labels(&record.name) {
            return Err(GoatNsError::Generic(format!(
                "At least one label is of length over 63 in name {}! I'm refusing to serve this record.",
                record.name
            )));
        };

        if record.name.len() > 255 {
            return Err(GoatNsError::Generic(format!(
                "The length of name ({}) is over 255 octets! ({}) I'm refusing to serve this record.",
                record.name,
                record.name.len()
            )));
        };

        Self::new(
            &record.name,
            record.rrtype,
            &record.rdata,
            record.ttl,
            record.rclass,
        )
    }
}

impl PartialEq<RecordClass> for InternalResourceRecord {
    fn eq(&self, other: &RecordClass) -> bool {
        match self {
            InternalResourceRecord::TXT { class, .. } => class == other,
            _ => other == &RecordClass::Internet, // TODO: we only support IN records outside TXT records
        }
    }
}

impl PartialEq<RecordType> for InternalResourceRecord {
    fn eq(&self, other: &RecordType) -> bool {
        match self {
            InternalResourceRecord::A { .. } => other == &RecordType::A,
            InternalResourceRecord::AAAA { .. } => other == &RecordType::AAAA,
            InternalResourceRecord::AXFR { .. } => other == &RecordType::AXFR,
            InternalResourceRecord::CAA { .. } => other == &RecordType::CAA,
            InternalResourceRecord::CDNSKEY { .. } => other == &RecordType::CDNSKEY,
            InternalResourceRecord::CDS { .. } => other == &RecordType::CDS,
            InternalResourceRecord::CNAME { .. } => other == &RecordType::CNAME,
            InternalResourceRecord::DNSKEY { .. } => other == &RecordType::DNSKEY,
            InternalResourceRecord::DS { .. } => other == &RecordType::DS,
            InternalResourceRecord::HINFO { .. } => other == &RecordType::HINFO,
            InternalResourceRecord::InvalidType => other == &RecordType::InvalidType,
            InternalResourceRecord::LOC { .. } => other == &RecordType::LOC,
            InternalResourceRecord::MX { .. } => other == &RecordType::MX,
            InternalResourceRecord::NAPTR { .. } => other == &RecordType::NAPTR,
            InternalResourceRecord::NSEC { .. } => other == &RecordType::NSEC,
            InternalResourceRecord::NSEC3 { .. } => other == &RecordType::NSEC3,
            InternalResourceRecord::NSEC3PARAM { .. } => other == &RecordType::NSEC3PARAM,
            InternalResourceRecord::NS { .. } => other == &RecordType::NS,
            InternalResourceRecord::PTR { .. } => other == &RecordType::PTR,
            InternalResourceRecord::RRSIG { .. } => other == &RecordType::RRSIG,
            InternalResourceRecord::SOA { .. } => other == &RecordType::SOA,
            InternalResourceRecord::TXT { .. } => other == &RecordType::TXT,
            InternalResourceRecord::URI { .. } => other == &RecordType::URI,
        }
    }
}

impl InternalResourceRecord {
    pub fn as_bytes(
        self: &InternalResourceRecord,
        question: &Vec<u8>,
    ) -> Result<Vec<u8>, GoatNsError> {
        match self {
            InternalResourceRecord::A { address, .. } => Ok(address.to_be_bytes().to_vec()),
            InternalResourceRecord::AAAA { address, .. } => Ok(address.to_be_bytes().to_vec()),

            InternalResourceRecord::CNAME { cname, .. } => {
                trace!("turning CNAME {cname:?} into bytes");
                Ok(cname
                    .as_bytes_compressed(Some(HEADER_BYTES as u16), Some(question))?
                    .into())
            }
            InternalResourceRecord::LOC {
                ttl,
                version,
                size,
                horiz_pre,
                vert_pre,
                latitude,
                longitude,
                altitude,
                ..
            } => {
                error!("LOC {:?} - TTL={ttl}", from_utf8(question));
                Ok(LocRecord {
                    version: *version,
                    size: *size,
                    horiz_pre: *horiz_pre,
                    vert_pre: *vert_pre,
                    latitude: *latitude,
                    longitude: *longitude,
                    altitude: *altitude,
                }
                .pack_to_vec()?)
            }
            InternalResourceRecord::NS { nsdname, .. } => Ok(nsdname
                .as_bytes_compressed(Some(HEADER_BYTES as u16), Some(question))?
                .into()),
            InternalResourceRecord::PTR { ptrdname, .. } => Ok(ptrdname
                .as_bytes_compressed(Some(HEADER_BYTES as u16), Some(question))?
                .into()),
            InternalResourceRecord::SOA {
                zone,
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
                ..
            } => {
                let zone_as_bytes = zone.name.as_bytes().to_vec();
                let mut res = mname
                    .as_bytes_compressed(Some(HEADER_BYTES as u16), Some(&zone_as_bytes))?
                    .as_bytes_with_trailing_null();
                res.extend(
                    rname
                        .as_bytes_compressed(Some(HEADER_BYTES as u16), Some(&zone_as_bytes))?
                        .as_bytes_with_trailing_null(),
                );
                res.extend(serial.to_be_bytes());
                res.extend(refresh.to_be_bytes());
                res.extend(retry.to_be_bytes());
                res.extend(expire.to_be_bytes());
                res.extend(minimum.to_be_bytes());
                Ok(res)
            }
            InternalResourceRecord::TXT { txtdata, .. } => Ok(txtdata.as_bytes()),
            InternalResourceRecord::URI {
                priority,
                weight,
                target,
                ..
            } => {
                let mut res = vec![];
                res.extend(priority.to_be_bytes());
                res.extend(weight.to_be_bytes());
                res.extend(&target.data);
                Ok(res)
            }
            InternalResourceRecord::HINFO { cpu, os, .. } => {
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
                Ok(hinfo_bytes)
            }
            // InternalResourceRecord::MINFO { ttl, .. } => ttl(),
            InternalResourceRecord::MX {
                preference,
                exchange,
                ..
            } => {
                let mut mx_bytes: Vec<u8> = preference.to_be_bytes().into();
                mx_bytes.extend(Into::<Vec<u8>>::into(
                    exchange.as_bytes_compressed(Some(HEADER_BYTES as u16), Some(question))?,
                ));
                Ok(mx_bytes)
            }
            InternalResourceRecord::AXFR { .. } => unimplemented!(), // TODO: handle axfr records
            InternalResourceRecord::InvalidType => {
                error!("Somehow people are requesting InvalidType records as bytes!");
                Ok(vec![])
            }
            InternalResourceRecord::CAA {
                flag, tag, value, ..
            } => {
                let mut result: Vec<u8> = vec![*flag];
                // add the tag
                result.extend(tag.as_bytes());
                // add the value
                result.extend(value);

                Ok(result)
            }
            InternalResourceRecord::NAPTR { .. } => {
                error!("Asked for an NAPTR as_bytes, returning null");
                Ok(Vec::new())
            }
            InternalResourceRecord::DNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                ..
            } => {
                let mut res = vec![];
                res.extend(flags.to_be_bytes());
                res.push(*protocol);
                res.push(*algorithm);
                res.extend(public_key);
                Ok(res)
            }
            InternalResourceRecord::RRSIG {
                type_covered,
                algorithm,
                labels,
                original_ttl,
                signature_expiration,
                signature_inception,
                key_tag,
                signer_name,
                signature,
                ..
            } => {
                let mut res = vec![];
                let rrtype: u16 = (*type_covered).into();
                res.extend(rrtype.to_be_bytes());
                res.push(*algorithm);
                res.push(*labels);
                res.extend(original_ttl.to_be_bytes());
                res.extend(signature_expiration.to_be_bytes());
                res.extend(signature_inception.to_be_bytes());
                res.extend(key_tag.to_be_bytes());
                let signer_bytes =
                    signer_name.as_bytes_compressed(Some(HEADER_BYTES as u16), Some(question))?;
                res.extend::<Vec<u8>>(signer_bytes.into());
                res.extend(signature);
                Ok(res)
            }
            InternalResourceRecord::NSEC {
                next_domain_name,
                type_bit_maps,
                ..
            } => {
                let mut res: Vec<u8> = next_domain_name
                    .as_bytes_compressed(Some(HEADER_BYTES as u16), Some(question))?
                    .into();
                res.extend(type_bit_maps);
                Ok(res)
            }
            InternalResourceRecord::NSEC3 {
                hash_algorithm,
                flags,
                iterations,
                salt,
                next_hashed_owner_name,
                type_bit_maps,
                ..
            } => {
                let mut res = vec![];
                res.push(*hash_algorithm);
                res.push(*flags);
                res.extend(iterations.to_be_bytes());
                res.push(salt.len() as u8);
                res.extend(salt);
                res.push(next_hashed_owner_name.len() as u8);
                res.extend(next_hashed_owner_name);
                res.extend(type_bit_maps);
                Ok(res)
            }
            InternalResourceRecord::NSEC3PARAM {
                hash_algorithm,
                flags,
                iterations,
                salt,
                ..
            } => {
                let mut res = vec![];
                res.push(*hash_algorithm);
                res.push(*flags);
                res.extend(iterations.to_be_bytes());
                res.push(salt.len() as u8);
                res.extend(salt);
                Ok(res)
            }
            InternalResourceRecord::DS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                ..
            } => {
                let mut res = vec![];
                res.extend(key_tag.to_be_bytes());
                res.push(*algorithm);
                res.push(*digest_type);
                res.extend(digest);
                Ok(res)
            }
            InternalResourceRecord::CDNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                ..
            } => {
                let mut res = vec![];
                res.extend(flags.to_be_bytes());
                res.push(*protocol);
                res.push(*algorithm);
                res.extend(public_key);
                Ok(res)
            }
            InternalResourceRecord::CDS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                ..
            } => {
                let mut res = vec![];
                res.extend(key_tag.to_be_bytes());
                res.push(*algorithm);
                res.push(*digest_type);
                res.extend(digest);
                Ok(res)
            }
        }
    }

    pub fn hexdump(self) {
        if let Err(err) = hexdump(
            &self
                .as_bytes(&vec![])
                .inspect_err(|err| error!("Failed to convert to bytes: {:?}", err))
                .unwrap_or_default(),
        ) {
            error!("Failed to decode bytes: {:?}", err);
        };
    }

    pub fn ttl(&self) -> &u32 {
        match self {
            InternalResourceRecord::A { ttl, .. } => ttl,
            InternalResourceRecord::AAAA { ttl, .. } => ttl,
            InternalResourceRecord::AXFR { ttl, .. } => ttl,
            InternalResourceRecord::CAA { ttl, .. } => ttl,
            InternalResourceRecord::CDNSKEY { ttl, .. } => ttl,
            InternalResourceRecord::CDS { ttl, .. } => ttl,
            InternalResourceRecord::CNAME { ttl, .. } => ttl,
            InternalResourceRecord::DNSKEY { ttl, .. } => ttl,
            InternalResourceRecord::DS { ttl, .. } => ttl,
            InternalResourceRecord::LOC { ttl, .. } => ttl,
            InternalResourceRecord::NAPTR { ttl, .. } => ttl,
            InternalResourceRecord::NSEC { ttl, .. } => ttl,
            InternalResourceRecord::NSEC3 { ttl, .. } => ttl,
            InternalResourceRecord::NSEC3PARAM { ttl, .. } => ttl,
            InternalResourceRecord::NS { ttl, .. } => ttl,
            InternalResourceRecord::SOA { minimum, .. } => minimum,
            InternalResourceRecord::PTR { ttl, .. } => ttl,
            InternalResourceRecord::HINFO { ttl, .. } => ttl,
            InternalResourceRecord::MX { ttl, .. } => ttl,
            InternalResourceRecord::RRSIG { ttl, .. } => ttl,
            InternalResourceRecord::TXT { ttl, .. } => ttl,
            InternalResourceRecord::URI { ttl, .. } => ttl,
            InternalResourceRecord::InvalidType => &0,
        }
    }
}

pub trait SetTTL {
    fn set_ttl(self, ttl: u32) -> Self;
}

impl SetTTL for InternalResourceRecord {
    fn set_ttl(self, ttl: u32) -> Self {
        match self {
            Self::A {
                address, rclass, ..
            } => Self::A {
                ttl,
                address,
                rclass,
            },
            Self::AAAA {
                address, rclass, ..
            } => Self::AAAA {
                address,
                rclass,
                ttl,
            },
            Self::AXFR { rclass, .. } => Self::AXFR { ttl, rclass },
            Self::CAA {
                flag,
                tag,
                value,
                rclass,
                ..
            } => Self::CAA {
                flag,
                tag,
                value,
                rclass,
                ttl,
            },
            Self::LOC {
                rclass,
                version,
                size,
                horiz_pre,
                vert_pre,
                latitude,
                longitude,
                altitude,
                ..
            } => Self::LOC {
                ttl,
                rclass,
                version,
                size,
                horiz_pre,
                vert_pre,
                latitude,
                longitude,
                altitude,
            },
            Self::NAPTR {
                rclass,
                domain,
                order,
                preference,
                flags,
                ..
            } => Self::NAPTR {
                rclass,
                domain,
                order,
                preference,
                flags,
                ttl,
            },
            Self::NS {
                nsdname, rclass, ..
            } => Self::NS {
                nsdname,
                ttl,
                rclass,
            },
            Self::SOA {
                zone,
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                rclass,
                ..
            } => Self::SOA {
                zone,
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum: ttl,
                rclass,
            },
            Self::PTR {
                ptrdname, rclass, ..
            } => Self::PTR {
                ptrdname,
                ttl,
                rclass,
            },
            Self::HINFO {
                cpu,
                os,
                ttl,
                rclass,
                ..
            } => Self::HINFO {
                cpu,
                os,
                rclass,
                ttl,
            },
            Self::MX {
                preference,
                exchange,
                rclass,
                ..
            } => Self::MX {
                preference,
                exchange,
                ttl,
                rclass,
            },
            Self::CDNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                rclass,
                ..
            } => Self::CDNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                rclass,
                ttl,
            },
            Self::CDS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                rclass,
                ..
            } => Self::CDS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                rclass,
                ttl,
            },
            Self::CNAME { cname, rclass, .. } => Self::CNAME { cname, ttl, rclass },
            Self::DNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                rclass,
                ..
            } => Self::DNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                rclass,
                ttl,
            },
            Self::DS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                rclass,
                ..
            } => Self::DS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                rclass,
                ttl,
            },
            Self::NSEC {
                next_domain_name,
                type_bit_maps,
                rclass,
                ..
            } => Self::NSEC {
                next_domain_name,
                type_bit_maps,
                rclass,
                ttl,
            },
            Self::NSEC3 {
                hash_algorithm,
                flags,
                iterations,
                salt,
                next_hashed_owner_name,
                type_bit_maps,
                rclass,
                ..
            } => Self::NSEC3 {
                hash_algorithm,
                flags,
                iterations,
                salt,
                next_hashed_owner_name,
                type_bit_maps,
                rclass,
                ttl,
            },
            Self::NSEC3PARAM {
                hash_algorithm,
                flags,
                iterations,
                salt,
                rclass,
                ..
            } => Self::NSEC3PARAM {
                hash_algorithm,
                flags,
                iterations,
                salt,
                rclass,
                ttl,
            },
            Self::RRSIG {
                type_covered,
                algorithm,
                labels,
                original_ttl,
                signature_expiration,
                signature_inception,
                key_tag,
                signer_name,
                signature,
                rclass,
                ..
            } => Self::RRSIG {
                type_covered,
                algorithm,
                labels,
                original_ttl,
                signature_expiration,
                signature_inception,
                key_tag,
                signer_name,
                signature,
                rclass,
                ttl,
            },
            Self::TXT { txtdata, class, .. } => Self::TXT {
                txtdata,
                class,
                ttl,
            },
            Self::URI {
                priority,
                weight,
                target,
                rclass,
                ..
            } => Self::URI {
                priority,
                weight,
                target,
                rclass,
                ttl,
            },
            //  Self::InvalidType => &0,
            _ => {
                error!("Tried to set TTL on an invalid type! {:?}", self);
                self
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;
    use std::str::FromStr;

    use tracing::debug;
    use uuid::Uuid;

    use crate::enums::RecordType;
    use crate::{RecordClass, db::entities};

    use super::{DNSCharString, DomainName, InternalResourceRecord};
    #[test]
    fn eq_resourcerecord() {
        assert_eq!(
            InternalResourceRecord::A {
                address: 12345,
                ttl: 1,
                rclass: RecordClass::Internet
            },
            RecordType::A
        );
        assert_eq!(
            InternalResourceRecord::AAAA {
                address: 12345,
                ttl: 1,
                rclass: RecordClass::Internet
            },
            RecordType::AAAA
        );
    }

    #[test]
    fn resourcerecord_from_ipv6_string() {
        let fzr = entities::records::Model {
            name: "test".to_string(),
            rrtype: RecordType::AAAA as u16,
            rdata: String::from("1234:5678:cafe:beef:ca75:0:4b9:e94d"),
            ttl: Some(160u32),
            rclass: RecordClass::Internet as u16,
            zoneid: Uuid::nil(),
            id: Uuid::nil(),
        };
        debug!("fzr: {fzr:?}");
        let converted = match Ipv6Addr::from_str(&fzr.rdata) {
            Ok(value) => value,
            Err(err) => panic!("Failed to convert rdata to string: {err:?}"),
        };

        debug!("conversion: {:?}", converted);
        let rr: InternalResourceRecord = match fzr.try_into() {
            Ok(value) => value,
            Err(error) => panic!("Failed to get resource record: {error:?}"),
        };

        debug!("fzr->rr = {rr:?}");
        assert_eq!(rr, RecordType::AAAA);
        assert_eq!(
            rr.as_bytes(&vec![]).expect("failed to convert to bytes"),
            [
                18, 52, 86, 120, 202, 254, 190, 239, 202, 117, 0, 0, 4, 185, 233, 77
            ]
            .to_vec()
        );
    }
    #[test]
    fn dnscharstring() {
        let test: DNSCharString = "hello world".into();
        let testbytes: Vec<u8> = test.into();
        assert_eq!(testbytes[0], 11);
    }

    #[test]
    fn resourcerecord_txt() {
        let foo = InternalResourceRecord::TXT {
            txtdata: DNSCharString::from("Hello world"),
            ttl: 1,
            class: RecordClass::Internet,
        };
        if let InternalResourceRecord::TXT { txtdata, .. } = foo {
            let foo_bytes: Vec<u8> = txtdata.into();
            assert_eq!(foo_bytes[0], 11);
        };
    }

    #[test]
    fn dnssec_dnskey_as_bytes() {
        let rr = InternalResourceRecord::DNSKEY {
            flags: 257,
            protocol: 3,
            algorithm: 8,
            public_key: vec![0xde, 0xad, 0xbe, 0xef],
            ttl: 3600,
            rclass: RecordClass::Internet,
        };
        let empty = Vec::new();
        let bytes = rr.as_bytes(&empty).expect("DNSKEY as_bytes");
        assert_eq!(bytes[0..2], [0x01, 0x01]); // flags 257
        assert_eq!(bytes[2], 3); // protocol
        assert_eq!(bytes[3], 8); // algorithm
        assert_eq!(bytes[4..], [0xde, 0xad, 0xbe, 0xef]); // public key
    }

    #[test]
    fn dnssec_ds_as_bytes() {
        let rr = InternalResourceRecord::DS {
            key_tag: 12345,
            algorithm: 8,
            digest_type: 2,
            digest: vec![0xab, 0xcd, 0xef, 0x01],
            ttl: 3600,
            rclass: RecordClass::Internet,
        };
        let empty = Vec::new();
        let bytes = rr.as_bytes(&empty).expect("DS as_bytes");
        assert_eq!(bytes[0..2], [0x30, 0x39]); // key_tag 12345
        assert_eq!(bytes[2], 8); // algorithm
        assert_eq!(bytes[3], 2); // digest_type
        assert_eq!(bytes[4..], [0xab, 0xcd, 0xef, 0x01]); // digest
    }

    #[test]
    fn dnssec_nsec3param_as_bytes() {
        let rr = InternalResourceRecord::NSEC3PARAM {
            hash_algorithm: 1,
            flags: 0,
            iterations: 10,
            salt: vec![0xaa, 0xbb],
            ttl: 3600,
            rclass: RecordClass::Internet,
        };
        let empty = Vec::new();
        let bytes = rr.as_bytes(&empty).expect("NSEC3PARAM as_bytes");
        assert_eq!(bytes[0], 1); // hash_algorithm
        assert_eq!(bytes[1], 0); // flags
        assert_eq!(bytes[2..4], [0x00, 0x0a]); // iterations 10
        assert_eq!(bytes[4], 2); // salt length
        assert_eq!(bytes[5..], [0xaa, 0xbb]); // salt
    }

    #[test]
    fn dnssec_rrsig_as_bytes() {
        let rr = InternalResourceRecord::RRSIG {
            type_covered: RecordType::A,
            algorithm: 8,
            labels: 2,
            original_ttl: 3600,
            signature_expiration: 1700000000,
            signature_inception: 1699999000,
            key_tag: 54321,
            signer_name: DomainName::from("example.com"),
            signature: vec![0xca, 0xfe],
            ttl: 3600,
            rclass: RecordClass::Internet,
        };
        // use the same name as the signer so it compresses to a pointer (12 = HEADER_BYTES)
        let question = b"example.com".to_vec();
        let bytes = rr.as_bytes(&question).expect("RRSIG as_bytes");
        // type_covered = A = 1
        assert_eq!(bytes[0..2], [0, 1]);
        // algorithm
        assert_eq!(bytes[2], 8);
        // labels
        assert_eq!(bytes[3], 2);
        // original_ttl
        assert_eq!(bytes[4..8], 3600u32.to_be_bytes());
        // signature_expiration
        assert_eq!(bytes[8..12], 1700000000u32.to_be_bytes());
        // signature_inception
        assert_eq!(bytes[12..16], 1699999000u32.to_be_bytes());
        // key_tag
        assert_eq!(bytes[16..18], 54321u16.to_be_bytes());
        // signer_name "example.com" compressed to a pointer [0xC0, 0x0C] (offset 12 = HEADER_BYTES)
        assert_eq!(bytes[18..20], [0xC0, 0x0C]);
        // signature follows the pointer
        assert_eq!(bytes[20..], [0xca, 0xfe]);
    }

    #[test]
    fn dnssec_dnskey_from_records_model() {
        let model = entities::records::Model {
            id: Uuid::nil(),
            zoneid: Uuid::nil(),
            name: "@".to_string(),
            ttl: Some(3600),
            rclass: RecordClass::Internet as u16,
            rrtype: RecordType::DNSKEY as u16,
            rdata: "01010308deadbeef".to_string(),
        };
        let rr: InternalResourceRecord = model.try_into().expect("DNSKEY try_from");
        match rr {
            InternalResourceRecord::DNSKEY {
                flags,
                protocol,
                algorithm,
                public_key,
                ..
            } => {
                assert_eq!(flags, 257);
                assert_eq!(protocol, 3);
                assert_eq!(algorithm, 8);
                assert_eq!(public_key, vec![0xde, 0xad, 0xbe, 0xef]);
            }
            _ => panic!("Expected DNSKEY variant"),
        }
    }

    #[test]
    fn dnssec_ds_from_records_model() {
        let model = entities::records::Model {
            id: Uuid::nil(),
            zoneid: Uuid::nil(),
            name: "@".to_string(),
            ttl: Some(3600),
            rclass: RecordClass::Internet as u16,
            rrtype: RecordType::DS as u16,
            rdata: "30390802abcdef01".to_string(),
        };
        let rr: InternalResourceRecord = model.try_into().expect("DS try_from");
        match rr {
            InternalResourceRecord::DS {
                key_tag,
                algorithm,
                digest_type,
                digest,
                ..
            } => {
                assert_eq!(key_tag, 12345);
                assert_eq!(algorithm, 8);
                assert_eq!(digest_type, 2);
                assert_eq!(digest, vec![0xab, 0xcd, 0xef, 0x01]);
            }
            _ => panic!("Expected DS variant"),
        }
    }

    #[test]
    fn dnssec_nsec3_from_records_model() {
        // NSEC3PARAM-like: hash_alg=1, flags=0, iterations=10, salt=[aa,bb]
        let model = entities::records::Model {
            id: Uuid::nil(),
            zoneid: Uuid::nil(),
            name: "@".to_string(),
            ttl: Some(3600),
            rclass: RecordClass::Internet as u16,
            rrtype: RecordType::NSEC3PARAM as u16,
            rdata: "0100000a02aabb".to_string(),
        };
        let rr: InternalResourceRecord = model.try_into().expect("NSEC3PARAM try_from");
        match rr {
            InternalResourceRecord::NSEC3PARAM {
                hash_algorithm,
                flags,
                iterations,
                salt,
                ..
            } => {
                assert_eq!(hash_algorithm, 1);
                assert_eq!(flags, 0);
                assert_eq!(iterations, 10);
                assert_eq!(salt, vec![0xaa, 0xbb]);
            }
            _ => panic!("Expected NSEC3PARAM variant"),
        }
    }

    #[test]
    fn dnssec_nsec_as_bytes() {
        let rr = InternalResourceRecord::NSEC {
            next_domain_name: DomainName::from("example.com"),
            type_bit_maps: vec![0x00, 0x04, 0x00, 0x00, 0x00, 0x00],
            ttl: 3600,
            rclass: RecordClass::Internet,
        };
        // use the same name as the reference so the next_domain_name compresses to a pointer
        let question = b"example.com".to_vec();
        let bytes = rr.as_bytes(&question).expect("NSEC as_bytes");
        // next_domain_name = "example.com" matches the reference, so it's compressed to [0xC0, 0x0C]
        assert_eq!(bytes[0..2], [0xC0, 0x0C]);
        // type_bit_maps follow the pointer
        assert_eq!(bytes[2..], [0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
    }
}

#[derive(PartialEq)]
/// This represents a LOC record in a zone file
pub struct FileLocRecord {
    pub d1: u8,
    pub d2: u8,
    pub m1: u8,
    pub m2: u8,
    pub s1: f32,
    pub s2: f32,
    pub lat_dir: String,
    pub lon_dir: String,
    pub alt: i32,
    pub size: u8,
    pub horiz_pre: u8,
    pub vert_pre: u8,
}

impl Default for FileLocRecord {
    fn default() -> Self {
        Self {
            d1: 0,
            m1: 0,
            s1: 0.0,
            d2: 0,
            m2: 0,
            s2: 0.0,
            lat_dir: Default::default(),
            lon_dir: Default::default(),
            alt: Default::default(),
            size: 0x12,      // 1m (100cm)
            horiz_pre: 0x16, // 10000m (1,000,000 cm)
            vert_pre: 0x13,  // 10m (1,000cm)
        }
    }
}

impl Debug for FileLocRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileLocRecord")
            .field("d1", &self.d1)
            .field("m1", &self.m1)
            .field("s1", &self.s1)
            .field("lat_dir", &self.lat_dir)
            .field("d2", &self.d2)
            .field("m2", &self.m2)
            .field("s2", &self.s2)
            .field("lon_dir", &self.lon_dir)
            .field("alt", &self.alt)
            .field("size", &format_args!("0x{:2x}", &self.size))
            .field("horiz_pre", &format_args!("0x{:2x}", &self.horiz_pre))
            .field("vert_pre", &format_args!("0x{:2x}", &self.vert_pre))
            .finish()
    }
}

impl TryFrom<&str> for FileLocRecord {
    type Error = GoatNsError;

    fn try_from(input_string: &str) -> Result<FileLocRecord, Self::Error> {
        // Thanks to the folks from #regex on Liberachat
        let loc_regex = Regex::new(
            r"^(?P<d1>\d+)(?:[ ](?P<m1>\d+)(?:[ ](?P<s1>\d+(?:[.]\d+)?))?)?[ ](?P<lat_dir>[NS])[ ](?P<d2>\d+)(?:[ ](?P<m2>\d+)(?:[ ](?P<s2>\d+(?:[.]\d+)?))?)?[ ](?P<lon_dir>[EW])[ ](?P<alt>-?\d+(?:[.]\d+)?)m(?:[ ](?P<size>\d+(?:[.]\d+)?)m(?:[ ](?P<hp>\d+(?:[.]\d+)?)m(?:[ ](?P<vp>\d+(?:[.]\d+)?)m)?)?)?",
        )?;

        let result = match loc_regex.captures(input_string) {
            Some(value) => value,
            None => {
                return Err(GoatNsError::Generic(
                    "Failed to match input to expected format!".to_string(),
                ));
            }
        };
        trace!("{result:?}");

        let d1: u8 = match result.name("d1") {
            Some(value) => match value.as_str().parse::<u8>() {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to parse d1: {err:?}");
                    0
                }
            },
            None => 0,
        };
        let d2: u8 = match result.name("d2") {
            Some(value) => match value.as_str().parse::<u8>() {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to parse d2: {err:?}");
                    0
                }
            },
            None => 0,
        };

        let m1: u8 = match result.name("m1") {
            Some(value) => match value.as_str().parse::<u8>() {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to parse m1: {err:?}");
                    FileLocRecord::default().m1
                }
            },
            None => FileLocRecord::default().m1,
        };
        let m2: u8 = match result.name("m2") {
            Some(value) => match value.as_str().parse::<u8>() {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to parse m2: {err:?}");
                    FileLocRecord::default().m2
                }
            },
            None => FileLocRecord::default().m2,
        };
        let s1: f32 = match result.name("s1") {
            Some(value) => match f32::from_str_radix(value.as_str(), 10) {
                Ok(value) => {
                    trace!("Parsed s1 as {value} from string");
                    value
                }
                Err(err) => {
                    error!("Failed to parse s1: {err:?}");
                    0.0
                }
            },
            None => 0f32,
        };
        let s2: f32 = match result.name("s2") {
            Some(value) => match f32::from_str_radix(value.as_str(), 10) {
                Ok(value) => value,
                Err(err) => {
                    error!("Failed to parse s2: {err:?}");
                    0.0
                }
            },
            None => 0f32,
        };
        let lat_dir: String = match result.name("lat_dir") {
            Some(value) => value.as_str().into(),
            None => {
                return Err(GoatNsError::Generic(
                    "Couldn't match lat_dir in this string!".to_string(),
                ));
            }
        };

        let lon_dir: String = match result.name("lon_dir") {
            Some(value) => value.as_str().into(),
            None => {
                return Err(GoatNsError::Generic(
                    "Couldn't match lon_dir in this string!".to_string(),
                ));
            }
        };

        let alt: i32 = match result.name("alt") {
            Some(value) => match value.as_str().parse::<i32>() {
                Ok(value) => value,
                Err(err) => {
                    return Err(GoatNsError::Generic(format!(
                        "Error parsing altitude: {err:?}"
                    )));
                }
            },
            None => return Err(GoatNsError::Generic("Error finding altitude!".to_string())),
        };
        // here we work out the final value for the altitude
        // let altfrac = alt_num % 100;
        // let altmeters = alt_num ;
        let alt = 10000000 + (alt * 100);

        let size: u32 = match result.name("size") {
            Some(value) => match value.as_str().parse::<u32>() {
                Ok(value) => {
                    trace!("Parsed size as {value} from string");
                    value
                }
                Err(err) => {
                    return Err(GoatNsError::Generic(format!(
                        "Failed to parse size: {value:?}, {err:?}"
                    )));
                }
            },
            None => {
                trace!("defaulting to size of 1");
                DEFAULT_LOC_SIZE
            }
        };
        let size = crate::utils::loc_size_to_u8(size as f32);

        let horiz_pre: u32 = match result.name("hp") {
            Some(value) => match value.as_str().parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    warn!("Failed to parse {value:?} as horizontal precision, using default");
                    DEFAULT_LOC_HORIZ_PRE
                }
            },
            None => {
                trace!("Using horiz_pre default as it wasn't specified");
                DEFAULT_LOC_HORIZ_PRE
            }
        };
        let horiz_pre = crate::utils::loc_size_to_u8(horiz_pre as f32);
        let vert_pre: u32 = match result.name("vp") {
            Some(value) => match value.as_str().parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    warn!("Failed to parse {value:?} as vertical precision, using default");
                    DEFAULT_LOC_VERT_PRE
                }
            },

            None => {
                trace!("Using vert_pre default as it wasn't specified");
                DEFAULT_LOC_VERT_PRE
            }
        };
        let vert_pre = crate::utils::loc_size_to_u8(vert_pre as f32);
        Ok(FileLocRecord {
            d1,
            d2,
            m1,
            m2,
            s1,
            s2,
            lat_dir,
            lon_dir,
            alt,
            size,
            horiz_pre,
            vert_pre,
        })
    }
}

/// tests to ensure that no label in the name is longer than 63 octets (bytes)
pub fn check_long_labels(testval: &str) -> bool {
    testval.split('.').any(|x| x.len() > 63)
}

#[tokio::test]
async fn test_soa_record() {
    let soa = InternalResourceRecord::SOA {
        zone: DomainName::from("example.com"),
        mname: DomainName::from("ns1.example.com"),
        rname: DomainName::from("hostmasterw.example.com"),
        serial: 123456789,
        refresh: 7200,
        retry: 3600,
        expire: 604800,
        minimum: 86400,
        rclass: RecordClass::Internet,
    };
    assert_eq!(soa.ttl(), &86400);
    let bytes = soa
        .as_bytes(&vec![])
        .expect("Failed to convert SOA record to bytes");

    // Expected bytes structure for SOA record:
    // - mname as domain name bytes (with trailing null)
    // - rname as domain name bytes (with trailing null)
    // - serial, refresh, retry, expire, minimum as u32 be bytes

    // Check the domain name format for mname (ns1.example.com)
    assert_eq!(bytes[0], 3); // length of 'ns1'
    assert_eq!(&bytes[1..4], b"ns1");
    assert_eq!(bytes[4], 192); // because of compression

    // Check the domain name format for rname (hostmaster.example.com)
    assert_eq!(bytes[18..=19], [192, 12]); // because of compression

    // Check the serial, refresh, retry, expire, minimum values
    assert_eq!(&bytes[20..24], [7, 91, 205, 21]); // serial
    assert_eq!(&bytes[24..28], [0, 0, 28, 32]); // refresh
    assert_eq!(&bytes[28..32], [0, 0, 14, 16]); // retry
    assert_eq!(&bytes[32..36], [0, 9, 58, 128]); // expire
    assert_eq!(&bytes[36..40], [0, 1, 81, 128]); // minimum

    assert_eq!(bytes.len(), 40); // Ensure there are more bytes after the minimum
}

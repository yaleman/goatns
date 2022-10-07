use crate::utils::name_as_bytes;
use crate::HEADER_BYTES;
/// RData field types
use log::error;
use packed_struct::prelude::*;
use serde::{Deserialize, Serialize};
use std::net::{AddrParseError, Ipv6Addr};
use std::str::FromStr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainName {
    name: String,
}

impl DomainName {
    /// Push the DomainName through the name_as_bytes function
    // TODO:
    pub fn as_bytes(&self, compress_target: Option<u16>) -> Vec<u8> {
        name_as_bytes(self.name.as_bytes().to_vec(), compress_target)
    }
}

#[allow(dead_code)]
#[derive(Deserialize, Serialize, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "4")]
/// An A Resource Record's RDATA is always 32 bits
pub struct RdataA {
    #[packed_field(bits = "0..=31", endian = "msb")]
    pub address: [u8; 4],
}

impl From<Vec<u8>> for RdataA {
    fn from(src: Vec<u8>) -> Self {
        let data = std::str::from_utf8(&src).unwrap();
        let data = match std::net::Ipv4Addr::from_str(data) {
            Ok(value) => value.octets().to_vec(),
            Err(error) => {
                error!(
                    "Failed to parse {:?} into an IPv4 address: {:?}",
                    data, error
                );
                vec![]
            }
        };
        let mut address: [u8; 4] = [0; 4];
        address.copy_from_slice(&data[0..4]);
        RdataA { address }
    }
}

#[allow(clippy::upper_case_acronyms, dead_code)]
#[derive(Deserialize, Serialize, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "16")]
/// An AAAA Resource Record's RDATA is always 128 bits
pub struct RdataAAAA {
    #[packed_field(bits = "0..=15", endian = "msb")]
    pub address: [u8; 16],
}

impl From<Vec<u8>> for RdataAAAA {
    fn from(src: Vec<u8>) -> Self {
        let address = std::str::from_utf8(&src).unwrap();
        let ipaddr: Result<Ipv6Addr, AddrParseError> = address.parse();
        let data = match ipaddr {
            Ok(value) => value.octets().to_vec(),
            Err(error) => {
                error!("Failed to parse {} to ipv6: {:?}", address, error);
                vec![]
            }
        };
        let mut address: [u8; 16] = [0; 16];
        address.copy_from_slice(&data[0..16]);
        Self { address }
    }
}

#[cfg(test)]
mod test {

    // use std::net::Ipv6Addr;

    // use super::RdataAAAA;

    #[test]
    fn test_aaaa() {
        // let testipv6 = Ipv6Addr::from("2001::b33f".as_bytes());
        // let testrecord = RdataAAAA{
        // rdata: testipv6.octets()
        // };
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "22")]
pub struct RdataSOAFields {
    // mname: Vec<u8>,
    // rname: Vec<u8>,
    #[packed_field(endian = "msb")]
    serial: u32,
    #[packed_field(endian = "msb")]
    refresh: u32,
    #[packed_field(endian = "msb")]
    retry: u32,
    #[packed_field(endian = "msb")]
    expire: u32,
    #[packed_field(endian = "msb")]
    minimum: u32,
}

impl From<&RdataSOA> for RdataSOAFields {
    fn from(source: &RdataSOA) -> Self {
        RdataSOAFields {
            serial: source.serial,
            refresh: source.refresh,
            retry: source.retry,
            expire: source.expire,
            minimum: source.minimum,
        }
    }
}

#[derive(Clone)]
pub struct RdataSOA {
    // TODO: change this to a DomainName
    pub mname: Vec<u8>,
    // TODO: change this to a DomainName
    pub rname: Vec<u8>,
    pub serial: u32,
    pub refresh: u32,
    pub retry: u32,
    pub expire: u32,
    pub minimum: u32,
}

impl RdataSOA {
    #[allow(dead_code)]
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut retval: Vec<u8> = vec![];
        retval.extend(name_as_bytes(self.mname.clone(), Some(HEADER_BYTES as u16)));
        retval.extend(name_as_bytes(self.rname.clone(), None));
        let simple_fields: RdataSOAFields = self.into();
        retval.extend(simple_fields.pack().unwrap());
        retval
    }
}

#[allow(dead_code)]
pub struct RdataCNAME {
    /// A <domain-name> which specifies the canonical or primary name for the owner.  The owner name is an alias.
    cname: Vec<u8>,
}

#[allow(dead_code)]
impl RdataCNAME {
    pub fn as_bytes(&self) -> Vec<u8> {
        vec![]
    }
}

#[allow(dead_code)]
pub struct RdataMX {
    // PREFERENCE      A 16 bit integer which specifies the preference given to this RR among others at the same owner.  Lower values are preferred.
    preference: u16,
    // EXCHANGE        A <domain-name> which specifies a host willing to act as a mail exchange for the owner name.
    exchange: DomainName,
}

#[allow(dead_code)]
impl RdataMX {
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut result: Vec<u8> = vec![];
        result.extend(self.preference.to_be_bytes());
        // TODO: support compresion in MX exchange fields
        result.extend(self.exchange.as_bytes(None));
        result
    }
}

#[allow(dead_code)]
pub struct RdataNS {
    // NSDNAME         A <domain-name> which specifies a host which should be authoritative for the specified class and domain.
    nsdname: DomainName,
}

#[allow(dead_code)]
impl RdataNS {
    pub fn as_bytes(&self) -> Vec<u8> {
        // TODO: support compression on this
        self.nsdname.as_bytes(None)
    }
}

#[allow(dead_code)]
pub struct RdataPTR {
    // PTRDNAME        A <domain-name> which points to some location in the domain name space.
    ptrdname: DomainName,
}

#[allow(dead_code)]
impl RdataPTR {
    pub fn as_bytes(&self) -> Vec<u8> {
        // TODO: support compression on this
        self.ptrdname.as_bytes(None)
    }
}

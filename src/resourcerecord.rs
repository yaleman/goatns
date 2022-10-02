use packed_struct::prelude::*;
use serde::{Deserialize, Serialize};

use crate::HEADER_BYTES;
use crate::utils::name_as_bytes;

#[allow(clippy::upper_case_acronyms)]
#[allow(dead_code)]
/// byte size of resource records
pub enum RRSize {
    A = 4,
    AAAA = 16,
}

#[allow(clippy::upper_case_acronyms, dead_code)]
#[derive(Deserialize, Serialize, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "16")]
/// An AAAA Resource Record's RDATA is always 128 bits
pub struct RdataAAAA {
    #[packed_field(bits = "0..=15", endian = "msb")]
    rdata: [u8; 16],
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


#[derive(Serialize, Deserialize, Eq, PartialEq,PackedStruct)]
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

impl From<RdataSOA> for RdataSOAFields {
    fn from(source: RdataSOA) -> Self {
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
    pub mname: Vec<u8>,
    pub rname: Vec<u8>,
    pub serial: u32,
    pub refresh: u32,
    pub retry: u32,
    pub expire: u32,
    pub minimum: u32,
}

impl RdataSOA {
    #[allow(dead_code)]
    pub fn as_bytes(self) -> Vec<u8>{
        let mut retval: Vec<u8> = vec![];
        retval.extend(name_as_bytes(self.mname.clone(), Some(HEADER_BYTES as u16)));
        retval.extend(name_as_bytes(self.rname.clone(), None));
        let simple_fields: RdataSOAFields = self.into();
        retval.extend(simple_fields.pack().unwrap());
        retval
    }
}
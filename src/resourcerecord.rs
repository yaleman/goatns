use packed_struct::prelude::*;
use serde::{Deserialize, Serialize};

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

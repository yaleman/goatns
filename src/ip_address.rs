/*
conversion functions
*/

// use std::net::Ipv6Addr;

use packed_struct::prelude::*;

#[derive(Debug, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "4")]
pub struct IPAddress {
    #[packed_field(bits = "0..=7", endian = "msb", element_size_bytes = "1")]
    oct_one: u8,
    #[packed_field(bits = "8..=15", endian = "msb", element_size_bytes = "1")]
    oct_two: u8,
    #[packed_field(bits = "16..=23", endian = "msb", element_size_bytes = "1")]
    oct_three: u8,
    #[packed_field(bits = "24..=31", endian = "msb", element_size_bytes = "1")]
    oct_four: u8,
}

impl IPAddress {
    #[allow(dead_code)]
    pub fn new(oct_one: u8, oct_two: u8, oct_three: u8, oct_four: u8) -> Self {
        IPAddress {
            oct_one,
            oct_two,
            oct_three,
            oct_four,
        }
    }
}

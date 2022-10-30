use serde::Serializer;
use std::net::{IpAddr, Ipv6Addr};

/// Convert a u32 to a string representation of an ipv4 address
pub fn a_to_ip<S>(address: &u32, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let addr_bytes = address.to_be_bytes();
    let addr = IpAddr::from(addr_bytes);
    s.serialize_str(&addr.to_string())
}

/// Convert a u128 to a string representation of an ipv6 address
pub fn aaaa_to_ip<S>(address: &u128, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let addr_bytes = address.to_be_bytes();
    let addr = Ipv6Addr::from(addr_bytes);
    s.serialize_str(&addr.to_string())
}

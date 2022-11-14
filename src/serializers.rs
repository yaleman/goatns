use serde::{de, Serializer};

use std::net::{IpAddr, Ipv6Addr};

use crate::enums::ContactDetails;

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


impl<'de> de::Deserialize<'de> for ContactDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        let res = ContactDetails::try_from(s.clone());
        log::trace!("deser input='{}' result='{:?}'", s, res);
        match res {
            Ok(val) => Ok(val),
            Err(err) => match err {
                crate::enums::ContactDetailsDeserializerError::InputLengthWrong { msg, len } => {
                    Err(de::Error::invalid_length(len, &msg))
                }
                crate::enums::ContactDetailsDeserializerError::InputFormatWrong { unexp, exp } => {
                    Err(de::Error::invalid_value(de::Unexpected::Str(&unexp), &exp))
                }
                crate::enums::ContactDetailsDeserializerError::WrongContactType(_msg) => {
                    todo!()
                }
            },
        }
    }
}

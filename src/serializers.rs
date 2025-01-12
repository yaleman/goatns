use serde::{de, Serializer};
use tracing::{error, trace};

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

/// Used for parsing the config file into a ContactDetails Object
impl<'de> de::Deserialize<'de> for ContactDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        let res = ContactDetails::try_from(s.clone());
        trace!("deser input='{}' result='{:?}'", s, res);
        match res {
            Ok(val) => Ok(val),
            Err(err) => match err {
                crate::enums::ContactDetailsDeserializerError::InputLengthWrong { msg, len } => {
                    Err(de::Error::invalid_length(len, &msg))
                }
                crate::enums::ContactDetailsDeserializerError::InputFormatWrong { unexp, exp } => {
                    Err(de::Error::invalid_value(de::Unexpected::Str(&unexp), &exp))
                }
                crate::enums::ContactDetailsDeserializerError::WrongContactType(msg) => {
                    error!("WrongContactType: '{}'", msg);
                    Err(de::Error::invalid_value(
                        de::Unexpected::Str(&msg),
                        &"Mastodon, Email, Twitter or None",
                    ))
                }
            },
        }
    }
}

impl serde::Serialize for ContactDetails {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string_repr = match self {
            ContactDetails::Mastodon { contact, server } => format!("Mastodon:{contact}@{server}",),
            ContactDetails::Email { contact } => format!("Email:{contact}",),
            ContactDetails::Twitter { contact } => format!("Twitter:{contact}",),
            ContactDetails::None => "".to_string(),
        };
        serializer.serialize_str(&string_repr)
    }
}

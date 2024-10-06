use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub static ref CAA_TAG_VALIDATOR: Regex =
        Regex::new(r"[a-zA-Z0-9]").expect("Failed to parse an internal regex!");
    pub static ref URI_RECORD: Regex =
        Regex::new(r"^(?P<priority>\d+) (?P<weight>\d+) (?P<target>.*)")
            .expect("Failed to parse an internal regex!");
}

pub fn dns_name(name: &str) -> bool {
    if name.len() > 253 {
        return false;
    }
    if name.is_empty() {
        return false;
    }

    for part in name.split('.') {
        if part.is_empty() {
            return false;
        }
        if part.len() > 63 {
            return false;
        }
        if !part.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return false;
        }
    }
    true
}

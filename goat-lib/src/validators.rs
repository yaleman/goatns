use once_cell::sync::Lazy;
use regex::Regex;

pub static CAA_TAG_VALIDATOR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[a-zA-Z0-9]").expect("Failed to parse an internal regex!"));
pub static URI_RECORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<priority>\d+) (?P<weight>\d+) (?P<target>.*)")
        .expect("Failed to parse an internal regex!")
});

pub fn dns_name(name: &str) -> bool {
    if !name.contains('.') {
        return false;
    }
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

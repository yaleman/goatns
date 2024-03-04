use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub static ref CAA_TAG_VALIDATOR: Regex =
        Regex::new(r"[a-zA-Z0-9]").expect("Failed to parse an internal regex!");
    pub static ref URI_RECORD: Regex =
        Regex::new(r"^(?P<priority>\d+) (?P<weight>\d+) (?P<target>.*)")
            .expect("Failed to parse an internal regex!");
}

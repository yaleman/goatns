use crate::error::GoatNsError;
use crate::zones::MAX_ZONE_FILE_SIZE;
use crate::zones::load_zones;
use std::io::Write;
use tempfile::NamedTempFile;

fn make_zone_json(name: &str, record_count: usize) -> String {
    let mut records = String::new();
    for i in 0..record_count {
        if i > 0 {
            records.push(',');
        }
        records.push_str(&format!(
            r#"{{"name":"rec{i}","rrtype":"A","rdata":"10.0.0.{i}","ttl":300}}"#
        ));
    }
    format!(
        r#"{{"name":"{name}","rname":"admin@{name}","serial":1,"refresh":3600,"retry":600,"expire":86400,"minimum":60,"records":[{records}]}}"#
    )
}

fn write_zone_file(entries: usize, record_count: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    write!(file, "[").expect("Failed to write");
    for i in 0..entries {
        if i > 0 {
            write!(file, ",").expect("Failed to write");
        }
        write!(
            file,
            "{}",
            make_zone_json(&format!("zone{i}.goat"), record_count)
        )
        .expect("Failed to write");
    }
    write!(file, "]").expect("Failed to write");
    file.flush().expect("Failed to flush");
    file
}

#[test]
fn test_small_zone_file() {
    let file = write_zone_file(2, 3);
    let result = load_zones(file.path().to_str().expect("Path to string"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");
    let zones = result.expect("zones");
    assert_eq!(zones.len(), 2);
    assert_eq!(zones[0].records.len(), 3);
}

#[test]
fn test_streaming_zone_file() {
    let entry_size = make_zone_json("zone0.goat", 100).len() as u64;
    let entries_needed = (MAX_ZONE_FILE_SIZE / entry_size) + 2;

    let file = write_zone_file(entries_needed as usize, 100);
    let file_size = file.path().metadata().expect("metadata").len();
    assert!(
        file_size > MAX_ZONE_FILE_SIZE,
        "Test file should exceed MAX_ZONE_FILE_SIZE, was {file_size}"
    );

    let result = load_zones(file.path().to_str().expect("Path to string"));
    assert!(
        result.is_ok(),
        "Expected Ok for streaming parse, got: {result:?}"
    );
    let zones = result.expect("zones");
    assert_eq!(zones.len(), entries_needed as usize);
}

#[test]
fn test_streaming_invalid_json() {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    let entry_size = make_zone_json("zone0.goat", 100).len() as u64;
    let entries_needed = (MAX_ZONE_FILE_SIZE / entry_size) + 2;

    write!(file, "[").expect("Failed to write");
    for i in 0..entries_needed {
        if i > 0 {
            write!(file, ",").expect("Failed to write");
        }
        write!(file, "{}", make_zone_json(&format!("zone{i}.goat"), 100)).expect("Failed to write");
    }
    write!(file, ",{{invalid}}]").expect("Failed to write");
    file.flush().expect("Failed to flush");

    let result = load_zones(file.path().to_str().expect("Path to string"));
    assert!(
        matches!(result, Err(GoatNsError::FileError(ref msg)) if msg.contains("streaming mode")),
        "Expected FileError with 'streaming mode', got: {result:?}"
    );
}

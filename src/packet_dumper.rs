use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error};

/// ```rust
/// use goatns::packet_dumper::{DumpType};
///
/// println!("Dumping type: {}", DumpType::ClientRequest);
/// ```
pub enum DumpType {
    ClientRequest,
    // Header
}

impl core::fmt::Display for DumpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            DumpType::ClientRequest => f.write_str("client_request"),
        }
    }
}

/// Dumps the bytes of a given vector to a templated packet
pub async fn dump_bytes(
    bytes: Vec<u8>,
    dump_type: DumpType,
    dest_dir: Option<PathBuf>,
) -> Option<String> {
    let now: DateTime<Utc> = Utc::now();

    debug!("bytes: {:?}", bytes);
    let filename = format!(
        "{}/{}-{}.cap",
        dest_dir
            .map(|f| f.display().to_string())
            .unwrap_or_else(|| "./captures".to_string()),
        dump_type,
        now.format("%Y-%m-%dT%H%M%SZ")
    );
    let mut fh = match File::create(&filename).await {
        Ok(value) => value,
        Err(error) => {
            error!("couldn't open {} for writing: {:?}", filename, error);
            return None;
        }
    };

    match fh.write_all(&bytes).await {
        Ok(_) => debug!("Successfully wrote packet to {}", &filename),
        Err(error) => debug!("Failed to write to {}: {:?}", filename, error),
    };
    Some(filename)
}

#[tokio::test]
async fn test_dump_bytes() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let dump_dir = dir.path().to_path_buf();
    let test_bytes: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let result = dump_bytes(test_bytes, DumpType::ClientRequest, Some(dump_dir))
        .await
        .expect("Should get filename");
    assert!(PathBuf::from(result).exists());
}

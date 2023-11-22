use chrono::{DateTime, Utc};
use log::debug;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub enum DumpType {
    ClientRequest,
}

impl core::fmt::Display for DumpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            DumpType::ClientRequest => f.write_str("client_request"),
        }
    }
}

/// Dumps the bytes of a given vector to a templated packet
pub async fn dump_bytes(bytes: Vec<u8>, dump_type: DumpType) {
    let now: DateTime<Utc> = Utc::now();

    debug!("bytes: {:?}", bytes);
    let filename = format!(
        "./captures/{}-{}.cap",
        dump_type,
        now.format("%Y-%m-%dT%H%M%SZ")
    );
    let mut fh = match File::create(&filename).await {
        Ok(value) => value,
        Err(error) => panic!("couldn't open {} for writing: {:?}", filename, error),
    };

    match fh.write_all(&bytes).await {
        Ok(_) => debug!("Successfully wrote packet to {}", &filename),
        Err(error) => debug!("Failed to write to {}: {:?}", filename, error),
    };
}

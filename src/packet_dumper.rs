use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use chrono::{DateTime, Utc};

pub async fn dump_bytes(bytes: Vec<u8>) {
    let now: DateTime<Utc> = Utc::now();

    eprintln!("bytes: {:?}", bytes);
    let filename = format!("./captures/{}", now.format("%Y-%m-%dT%H%M%SZ.cap"));
    let mut fh = match File::create(&filename).await {
        Ok(value) => value,
        Err(error) => panic!("couldn't open {:?}: {:?}", filename, error),
    };

    match fh.write_all(&bytes).await {
        Ok(_) => eprintln!("Successfully wrote packet to {:?}", &filename),
        Err(error) => eprintln!("Failed to write to {:?}: {:?}", filename, error),
    };
}

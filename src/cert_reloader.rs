use std::path::PathBuf;

use concread::cowcell::asynch::CowCellReadTxn;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::Sender;
use tracing::error;

use crate::{config::ConfigFile, error::GoatNsError, web::ServerCommand};

async fn hash_file(path: &PathBuf) -> Result<Vec<u8>, GoatNsError> {
    let mut file = File::open(&path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 4096];

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize().to_vec())
}

pub async fn cert_reloader(
    config: CowCellReadTxn<ConfigFile>,
    apiserver_tx: Sender<ServerCommand>,
) -> Result<(), GoatNsError> {
    use std::time::Duration;
    use tracing::{debug, info};

    info!("Starting certificate reloader task.");
    let mut interval = tokio::time::interval(Duration::from_secs(
        config.cert_reload_interval_seconds.unwrap_or(300),
    )); // defaults to every 5 minutes

    let mut current_api_tls_cert_hash = hash_file(&config.api_tls_cert).await?;
    let mut current_api_tls_key_hash = hash_file(&config.api_tls_key).await?;

    loop {
        interval.tick().await;
        debug!("Checking for TLS certificate updates...");
        let api_tls_cert_hash = hash_file(&config.api_tls_cert).await?;
        let api_tls_key_hash = hash_file(&config.api_tls_key).await?;
        // store the current cert hashes

        if (api_tls_cert_hash != current_api_tls_cert_hash)
            || (api_tls_key_hash != current_api_tls_key_hash)
        {
            if let Err(err) = apiserver_tx.send(ServerCommand::ReloadTls).await {
                error!("Failed to send message to server to reload TLS certs: {err}");
            } else {
                info!("TLS certificate or key file has changed, reloading...");
                current_api_tls_cert_hash = api_tls_cert_hash;
                current_api_tls_key_hash = api_tls_key_hash;
            }
        }
    }
}

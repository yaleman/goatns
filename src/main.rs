use log::{error, info, LevelFilter};
use std::io;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::sync::mpsc;

use goatns::config::{get_config, ConfigFile};
use goatns::datastore;
use goatns::servers;
use goatns::utils::*;
use goatns::MAX_IN_FLIGHT;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();
    let config: ConfigFile = get_config(clap_results.get_one::<String>("config"));

    let log_level = match LevelFilter::from_str(config.log_level.as_str()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "Failed to parse log level {:?} - {:?}. Reverting to debug",
                config.log_level.as_str(),
                error
            );

            LevelFilter::Debug
        }
    };
    femme::with_level(log_level);
    info!("Configuration: {}", config);
    let listen_addr = format!("{}:{}", config.address, config.port);

    let bind_address = match listen_addr.parse::<SocketAddr>() {
        Ok(value) => value,
        Err(error) => {
            error!("Failed to parse address: {:?}", error);
            return Ok(());
        }
    };

    let tx: mpsc::Sender<datastore::Command>;
    let rx: mpsc::Receiver<datastore::Command>;
    (tx, rx) = mpsc::channel(MAX_IN_FLIGHT);

    let datastore_manager = tokio::spawn(datastore::manager(rx, config.clone()));
    let udpserver = tokio::spawn(servers::udp_server(
        bind_address,
        config.clone(),
        tx.clone(),
    ));
    let tcpserver = tokio::spawn(servers::tcp_server(
        bind_address,
        config.clone(),
        tx.clone(),
    ));

    loop {
        // if any of the servers bail, the server does too.
        if udpserver.is_finished() || tcpserver.is_finished() || datastore_manager.is_finished() {
            return Ok(());
        }
        sleep(std::time::Duration::from_secs(1)).await;
    }
}

use log::{error, info};
use std::io;
use std::net::SocketAddr;
use tokio::sync::mpsc;

use goatns::config::{check_config, get_config, setup_logging, ConfigFile};
use goatns::datastore;
use goatns::servers;
use goatns::utils::*;
use goatns::MAX_IN_FLIGHT;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();
    let config: ConfigFile = get_config(clap_results.get_one::<String>("config"));

    let _logger = match setup_logging(&config, &clap_results) {
        Ok(logger) => logger,
        Err(_) => return Ok(()),
    };

    let config_check_result = check_config(&config);

    if clap_results.get_flag("configcheck") {
        log::info!("{:#?}", config);
        match config_check_result {
            Ok(_) => log::info!("Checking config... [OK!]"),
            Err(_) => log::error!("Checking config... [ERR!]"),
        };
    }

    // sometimes you just have to print some errors
    if let Err(errors) = config_check_result {
        for error in errors {
            log::error!("{error:}")
        }
        log::error!("Shutting down!");
        return Ok(());
    };
    if clap_results.get_flag("configcheck") {
        return Ok(());
    }

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

    // start all the things!
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

    let apiserver = match goatns::api::build(config, tx.clone()).await {
        Ok(value) => value,
        Err(err) => {
            log::error!("Failed to build API server: {err:?}");
            return Ok(());
        }
    };
    let api = tokio::spawn(apiserver.launch());

    loop {
        // if any of the servers bail, the server does too.
        if udpserver.is_finished()
            || tcpserver.is_finished()
            || datastore_manager.is_finished()
            || api.is_finished()
        {
            return Ok(());
        }
        sleep(std::time::Duration::from_secs(1)).await;
    }
}

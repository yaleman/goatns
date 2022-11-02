use log::{error, info};
use std::io;
use std::net::SocketAddr;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use goatns::config::{check_config, get_config, setup_logging, ConfigFile};
use goatns::datastore;
use goatns::enums::{Agent, AgentState};
use goatns::servers;
use goatns::utils::clap_parser;
use goatns::MAX_IN_FLIGHT;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();
    let config: ConfigFile = get_config(clap_results.get_one::<String>("config"));

    let logger = match setup_logging(&config, &clap_results) {
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

    let agent_tx: tokio::sync::broadcast::Sender<AgentState>;
    #[allow(unused_variables)]
    let _agent_rx: tokio::sync::broadcast::Receiver<AgentState>;
    (agent_tx, _agent_rx) = broadcast::channel(32);
    let tx: mpsc::Sender<datastore::Command>;
    let rx: mpsc::Receiver<datastore::Command>;
    (tx, rx) = mpsc::channel(MAX_IN_FLIGHT);

    // start all the things!
    let datastore_manager = tokio::spawn(datastore::manager(rx, config.clone()));

    if clap_results.get_one::<String>("export_zone").is_some() {
        let zone_name = clap_results.get_one::<String>("export_zone").unwrap();

        info!("Exporting zone {zone_name}");
        return Ok(());
    }

    // Let's start up the listeners!
    let udpserver = tokio::spawn(servers::udp_server(
        bind_address,
        config.clone(),
        tx.clone(),
        agent_tx.clone(),
        agent_tx.subscribe(),
    ));
    let tcpserver = tokio::spawn(servers::tcp_server(
        bind_address,
        config.clone(),
        tx.clone(),
        agent_tx.clone(),
        agent_tx.subscribe(),
    ));

    let api = match goatns::api::build(config, tx.clone()).await {
        Ok(value) => value,
        Err(err) => {
            log::error!("Failed to build API server: {err:?}");
            return Ok(());
        }
    };
    let apiserver = tokio::spawn(api.launch());

    loop {
        // if any of the servers bail, the server does too.
        if udpserver.is_finished() {
            log::info!("UDP Server shut down");
            if let Err(error) = agent_tx.send(AgentState::Stopped {
                agent: Agent::UDPServer,
            }) {
                eprintln!("Failed to send UDPServer shutdown message: {error:?}");
            };
            return Ok(());
        };
        if tcpserver.is_finished() {
            log::info!("TCP Server shut down");
            if let Err(error) = agent_tx.send(AgentState::Stopped {
                agent: Agent::TCPServer,
            }) {
                eprintln!("Failed to send UDPServer shutdown message: {error:?}");
            };
            return Ok(());
        };
        if datastore_manager.is_finished() {
            log::info!("Datastore manager shut down");
            if let Err(error) = agent_tx.send(AgentState::Stopped {
                agent: Agent::Datastore,
            }) {
                eprintln!("Failed to send UDPServer shutdown message: {error:?}");
            };
            return Ok(());
        };
        if apiserver.is_finished() {
            log::info!("API manager shut down");
            if let Err(error) = agent_tx.send(AgentState::Stopped { agent: Agent::API }) {
                eprintln!("Failed to send UDPServer shutdown message: {error:?}");
            };
            // return Ok(());
        }

        if udpserver.is_finished()
            & tcpserver.is_finished()
            & apiserver.is_finished()
            & datastore_manager.is_finished()
        {
            break;
        }
        sleep(std::time::Duration::from_secs(1)).await;
    }
    logger.flush();
    Ok(())
}

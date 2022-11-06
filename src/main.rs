use log::{error, info};
use std::io;
use std::net::SocketAddr;
use tide_rustls::TlsListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use goatns::config::{check_config, get_config, setup_logging, ConfigFile};
use goatns::datastore;
use goatns::enums::{Agent, AgentState, SystemState};
use goatns::servers;
use goatns::utils::clap_parser;
use goatns::MAX_IN_FLIGHT;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();
    let mut config: ConfigFile = get_config(clap_results.get_one::<String>("config"));

    let logger = match setup_logging(&config, &clap_results) {
        Ok(logger) => logger,
        Err(_) => return Ok(()),
    };

    let config = check_config(&mut config);

    if clap_results.get_flag("configcheck") {
        log::info!("{:#?}", config);
        match config {
            Ok(_) => log::info!("Checking config... [OK!]"),
            Err(_) => log::error!("Checking config... [ERR!]"),
        };
    }

    // sometimes you just have to print some errors
    let config = match config {
        Err(errors) => {
            for error in errors {
                log::error!("{error:}")
            }
            log::error!("Shutting down!");
            return Ok(());
        }
        Ok(c) => {
            if clap_results.get_flag("configcheck") {
                return Ok(());
            };
            c
        }
    };

    info!("Configuration: {}", config);

    let listen_addr = format!("{}:{}", config.address, config.port);

    let bind_address = match listen_addr.parse::<SocketAddr>() {
        Ok(value) => value,
        Err(error) => {
            error!("Failed to parse address: {:?}", error);
            return Ok(());
        }
    };

    // agent signalling
    let agent_tx: tokio::sync::broadcast::Sender<AgentState>;
    #[allow(unused_variables)]
    let _agent_rx: tokio::sync::broadcast::Receiver<AgentState>;
    (agent_tx, _agent_rx) = broadcast::channel(32);
    let tx: mpsc::Sender<datastore::Command>;
    let rx: mpsc::Receiver<datastore::Command>;
    (tx, rx) = mpsc::channel(MAX_IN_FLIGHT);

    // start all the things!
    let datastore_manager =
        tokio::spawn(datastore::manager(rx, config.clone(), clap_results.clone()));

    let system_state = match goatns::utils::cli_commands(tx.clone(), &clap_results).await {
        Ok(value) => value,
        Err(error) => {
            log::trace!("{error}");
            SystemState::ShuttingDown
        }
    };

    log::debug!("System state: {system_state:?}");
    // if we got this far we can shut down again
    match system_state {
        SystemState::Export | SystemState::Import | SystemState::ShuttingDown => {
            logger.flush();
            if let Err(error) = tx.send(datastore::Command::Shutdown).await {
                eprintln!("failed to tell Datastore to shut down! {error:?} Bailing!");
                logger.flush();
                return Ok(());
            };
        }
        SystemState::Server => {
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

            // let _apiserver: tokio::task::JoinHandle<Result<(),std::io::Error>>;
            if config.enable_api {
                let api_listener = match TlsListener::build()
                    .addrs(config.api_listener_address())
                    .cert(&config.api_tls_cert)
                    .key(&config.api_tls_key)
                    .finish()
                {
                    Ok(value) => value,
                    Err(error) => {
                        log::error!("Failed to start API TLS Listener: {error:?}");
                        return Ok(());
                    }
                };
                let api = match goatns::web::build(tx.clone(), &config.clone()).await {
                    Ok(value) => value,
                    Err(err) => {
                        // TODO: need to clean-shutdown the server here
                        log::error!("Failed to build API server: {err:?}");
                        return Ok(());
                    }
                };
                let _apiserver = tokio::spawn(api.listen(api_listener));
            }

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
                        eprintln!("Failed to send TCPServer shutdown message: {error:?}");
                    };
                    return Ok(());
                };
                // if config.enable_api {
                //     if apiserver.is_finished() {
                //         log::info!("API manager shut down");
                //         if let Err(error) = agent_tx.send(AgentState::Stopped { agent: Agent::API }) {
                //             eprintln!("Failed to send API Server shutdown message: {error:?}");
                //         };
                //     }
                // }

                if datastore_manager.is_finished() {
                    log::info!("Datastore manager shut down!");
                    if let Err(error) = agent_tx.send(AgentState::Stopped {
                        agent: Agent::Datastore,
                    }) {
                        eprintln!("Failed to send Datastore shutdown message: {error:?}");
                    };
                    return Ok(());
                };

                if udpserver.is_finished()
                    & tcpserver.is_finished()
                    & datastore_manager.is_finished()
                {
                    break;
                }
                sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
    logger.flush();
    sleep(std::time::Duration::from_secs(1)).await;
    logger.flush();
    Ok(())
}

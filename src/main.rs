use concread::cowcell::asynch::CowCell;
use goatns::enums::SystemState;
use goatns::utils::start_channels;
use sqlx::Pool;
use sqlx::Sqlite;
use std::io;
use std::io::Error;
use tokio::task::JoinHandle;

use tokio::sync::broadcast;
use tokio::sync::mpsc;

use goatns::cli::clap_parser;
use goatns::config::{setup_logging, ConfigFile};
use goatns::datastore;
use goatns::db;
use goatns::enums::{Agent, AgentState};
use goatns::servers;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();

    let config = ConfigFile::try_as_cowcell(clap_results.get_one::<String>("config"))?;

    let logger = setup_logging(config.read().await, &clap_results).await?;

    let config_result = ConfigFile::check_config(config.write().await).await;

    if clap_results.get_flag("configcheck") {
        log::info!("{}", config.read().await.as_json_pretty());
        match config_result {
            Ok(_) => log::info!("Checking config... [OK!]"),
            Err(_) => log::error!("Checking config... [ERR!]"),
        };
    }

    // sometimes you just have to print some errors
    if let Err(errors) = config_result {
        for error in errors {
            log::error!("{error:}")
        }
        log::error!("Shutting down!");
        logger.flush();
        sleep(std::time::Duration::from_millis(250)).await;
        logger.shutdown();
        return Ok(());
    };
    if clap_results.get_flag("configcheck") {
        return Ok(());
    };

    log::info!("Configuration: {}", *config.read().await);

    let (agent_tx, datastore_sender, datastore_receiver) = start_channels();

    // start up the DB
    let connpool = db::get_conn(config.read().await).await?;

    if let Err(err) = db::start_db(&connpool).await {
        log::error!("{err}");
        return Ok(());
    };

    // start all the things!
    let datastore_manager = tokio::spawn(datastore::manager(datastore_receiver, connpool.clone()));

    match goatns::cli::cli_commands(
        datastore_sender.clone(),
        &clap_results,
        &config.read().await.zone_file,
    )
    .await
    {
        Ok(resp) => {
            if resp == SystemState::Server {
                start(
                    config,
                    datastore_sender,
                    agent_tx,
                    connpool,
                    datastore_manager,
                )
                .await?
            }
        }
        Err(error) => {
            log::trace!("{error}")
        }
    };
    logger.flush();

    logger.shutdown();
    Ok(())
}

#[derive(Debug)]
struct Servers {
    pub datastore: Option<JoinHandle<Result<(), String>>>,
    pub udpserver: Option<JoinHandle<Result<(), Error>>>,
    pub tcpserver: Option<JoinHandle<Result<(), Error>>>,
    pub apiserver: Option<JoinHandle<Result<(), Error>>>,
    pub agent_tx: broadcast::Sender<AgentState>,
}

impl Default for Servers {
    fn default() -> Self {
        let (agent_tx, _) = broadcast::channel(10000);
        Self {
            datastore: None,
            udpserver: None,
            tcpserver: None,
            apiserver: None,
            agent_tx,
        }
    }
}

impl Servers {
    fn build(agent_tx: broadcast::Sender<AgentState>) -> Self {
        Self {
            agent_tx,
            ..Default::default()
        }
    }
    fn with_apiserver(self, apiserver: Option<JoinHandle<Result<(), Error>>>) -> Self {
        Self { apiserver, ..self }
    }
    fn with_datastore(self, datastore: JoinHandle<Result<(), String>>) -> Self {
        Self {
            datastore: Some(datastore),
            ..self
        }
    }
    fn with_tcpserver(self, tcpserver: JoinHandle<Result<(), Error>>) -> Self {
        Self {
            tcpserver: Some(tcpserver),
            ..self
        }
    }
    fn with_udpserver(self, udpserver: JoinHandle<Result<(), Error>>) -> Self {
        Self {
            udpserver: Some(udpserver),
            ..self
        }
    }

    fn send_shutdown(&self, agent: Agent) {
        log::info!("{agent:?} shut down");
        if let Err(error) = self.agent_tx.send(AgentState::Stopped { agent }) {
            eprintln!("Failed to send agent shutdown message: {error:?}");
        };
    }

    fn all_finished(&self) -> bool {
        let mut results = vec![];
        if let Some(server) = &self.apiserver {
            if server.is_finished() {
                self.send_shutdown(Agent::API);
            }
            results.push(server.is_finished())
        }
        if let Some(server) = &self.datastore {
            if server.is_finished() {
                self.send_shutdown(Agent::Datastore);
            }
            results.push(server.is_finished())
        }
        if let Some(server) = &self.tcpserver {
            if server.is_finished() {
                self.send_shutdown(Agent::TCPServer);
            }
            results.push(server.is_finished())
        }
        if let Some(server) = &self.udpserver {
            if server.is_finished() {
                self.send_shutdown(Agent::UDPServer);
            }
            results.push(server.is_finished())
        }
        results.iter().any(|&r| r)
    }
}

async fn start(
    config: CowCell<ConfigFile>,
    tx: mpsc::Sender<datastore::Command>,
    agent_tx: broadcast::Sender<AgentState>,
    connpool: Pool<Sqlite>,
    datastore_manager: JoinHandle<Result<(), String>>,
) -> io::Result<()> {
    // Let's start up the listeners!
    let udpserver = tokio::spawn(servers::udp_server(
        config.read().await,
        tx.clone(),
        agent_tx.clone(),
        agent_tx.subscribe(),
    ));
    let tcpserver = tokio::spawn(servers::tcp_server(
        config.read().await,
        tx.clone(),
        agent_tx.clone(),
        agent_tx.subscribe(),
    ));

    let apiserver = goatns::web::build(tx.clone(), config.read().await, connpool.clone()).await;

    let servers = Servers::build(agent_tx)
        .with_datastore(datastore_manager)
        .with_udpserver(udpserver)
        .with_tcpserver(tcpserver)
        .with_apiserver(apiserver);

    loop {
        if servers.all_finished() {
            break;
        }
        sleep(std::time::Duration::from_micros(500)).await;
    }
    Ok(())
}

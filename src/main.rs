use concread::cowcell::asynch::CowCell;
use flexi_logger::LoggerHandle;
use sqlx::Pool;
use sqlx::Sqlite;
use std::io;
use std::io::Error;
use tokio::task::JoinHandle;

use tokio::sync::broadcast;
use tokio::sync::mpsc;

use goatns::cli::clap_parser;
use goatns::config::{check_config, get_config_cowcell, setup_logging, ConfigFile};
use goatns::datastore;
use goatns::db;
use goatns::enums::{Agent, AgentState, SystemState};
use goatns::servers;
use goatns::MAX_IN_FLIGHT;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> io::Result<()> {
    let clap_results = clap_parser();
    let config = match get_config_cowcell(clap_results.get_one::<String>("config")) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    let logger = match setup_logging(config.read().await, &clap_results).await {
        Ok(logger) => logger,
        Err(_) => return Ok(()),
    };

    let config_result = check_config(config.write().await).await;
    if clap_results.get_flag("configcheck") {
        log::info!("{}", config.read().await.as_json_pretty());
        match config_result {
            Ok(_) => log::info!("Checking config... [OK!]"),
            Err(_) => log::error!("Checking config... [ERR!]"),
        };
    }

    // sometimes you just have to print some errors
    match check_config(config.write().await).await {
        Err(errors) => {
            for error in errors {
                log::error!("{error:}")
            }
            log::error!("Shutting down!");
            logger.flush();
            sleep(std::time::Duration::from_millis(250)).await;
            logger.flush();
            return Ok(());
        }
        Ok(value) => {
            if clap_results.get_flag("configcheck") {
                return Ok(());
            };
            value
        }
    };

    log::info!("Configuration: {}", *config.read().await);

    // agent signalling
    let agent_tx: broadcast::Sender<AgentState>;
    #[allow(unused_variables)]
    let _agent_rx: broadcast::Receiver<AgentState>;
    (agent_tx, _agent_rx) = broadcast::channel(32);
    let tx: mpsc::Sender<datastore::Command>;
    let rx: mpsc::Receiver<datastore::Command>;
    (tx, rx) = mpsc::channel(MAX_IN_FLIGHT);

    // start up the DB
    let connpool = match db::get_conn(config.read().await).await {
        Ok(value) => value,
        Err(err) => {
            log::error!("Failed to start sqlite connection tool: {err}");
            return Ok(());
        }
    };
    if let Err(err) = db::start_db(&connpool).await {
        log::error!("{err}");
        return Ok(());
    };

    // start all the things!
    let zone_file = config.read().await;
    let zone_file = zone_file.zone_file.clone();
    let datastore_manager = tokio::spawn(datastore::manager(
        rx,
        zone_file,
        clap_results.get_flag("use_zonefile"),
        connpool.clone(),
    ));

    let system_state = match goatns::cli::cli_commands(tx.clone(), &clap_results).await {
        Ok(value) => value,
        Err(error) => {
            log::trace!("{error}");
            SystemState::ShuttingDown
        }
    };

    start(
        logger,
        config,
        system_state,
        tx,
        agent_tx,
        connpool,
        datastore_manager,
    )
    .await?;

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
    logger: LoggerHandle,
    config: CowCell<ConfigFile>,
    system_state: SystemState,
    tx: mpsc::Sender<datastore::Command>,
    agent_tx: broadcast::Sender<AgentState>,
    connpool: Pool<Sqlite>,
    datastore_manager: JoinHandle<Result<(), String>>,
) -> io::Result<()> {
    log::debug!("System state: {system_state:?}");
    // if we got this far we can shut down again
    if system_state == SystemState::Server {
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
    }
    logger.flush();
    Ok(())
}

use goatns::enums::SystemState;
use goatns::utils::start_channels;
use std::io;
use std::time::Duration;

use goatns::cli::clap_parser;
use goatns::config::{setup_logging, ConfigFile};
use goatns::datastore;
use goatns::db;
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
        logger.shutdown();
        return Ok(());
    };
    if clap_results.get_flag("configcheck") {
        log::error!("Shutting down!");
        logger.shutdown();
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
    let datastore_manager = tokio::spawn(datastore::manager(
        datastore_receiver,
        connpool.clone(),
        Some(Duration::from_secs(
            config.read().await.sql_db_cleanup_seconds,
        )),
    ));

    match goatns::cli::cli_commands(
        datastore_sender.clone(),
        &clap_results,
        &config.read().await.zone_file,
    )
    .await
    {
        Ok(resp) => {
            if resp == SystemState::Server {
                let udpserver = tokio::spawn(servers::udp_server(
                    config.read().await,
                    datastore_sender.clone(),
                    agent_tx.clone(),
                    // agent_tx.subscribe(),
                ));
                let tcpserver = tokio::spawn(servers::tcp_server(
                    config.read().await,
                    datastore_sender.clone(),
                    agent_tx.clone(),
                    // agent_tx.subscribe(),
                ));

                let apiserver = goatns::web::build(
                    datastore_sender.clone(),
                    config.read().await,
                    connpool.clone(),
                )
                .await;

                let servers = servers::Servers::build(agent_tx)
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
        }
        Err(error) => {
            log::trace!("{error}")
        }
    };
    logger.flush();

    logger.shutdown();
    Ok(())
}

use goatns::enums::SystemState;
use goatns::utils::start_channels;
use sqlx::SqlitePool;
use std::io;
use std::time::Duration;

use goatns::cli::clap_parser;
use goatns::config::{setup_logging, ConfigFile};
use goatns::datastore;
use goatns::db;
use goatns::servers;
use tokio::time::sleep;

async fn run() -> Result<(), io::Error> {
    let clap_results = clap_parser();

    let config =
        ConfigFile::try_as_cowcell(clap_results.get_one::<String>("config")).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Config loading failed! {:?}", err),
            )
        })?;

    let logger = setup_logging(config.read(), &clap_results)
        .await
        .map_err(|err| {
            io::Error::new(io::ErrorKind::Other, format!("Log setup failed! {:?}", err))
        })?;

    let config_result = ConfigFile::check_config(config.write().await).await;

    if clap_results.get_flag("configcheck") {
        log::info!(
            "{}",
            config
                .read()
                .as_json_pretty()
                .expect("Failed to serialize config!")
        );
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
        log::error!("Shutting down due to error!");
        logger.shutdown();
        return Err(io::Error::new(io::ErrorKind::Other, "Config check failed!"));
    };

    if clap_results.get_flag("configcheck") {
        log::info!("Shutting down after config check.");
        logger.shutdown();
        return config_result
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Config check failed!"));
    };

    log::info!("Configuration: {}", *config.read());

    let (agent_tx, datastore_sender, datastore_receiver) = start_channels();

    // start up the DB
    let connpool: SqlitePool = db::get_conn(config.read()).await.map_err(|err| {
        io::Error::new(io::ErrorKind::Other, format!("DB Setup failed: {:?}", err))
    })?;

    if let Err(err) = db::start_db(&connpool).await {
        log::error!("{err}");
        return Ok(());
    };

    // start all the things!
    let datastore_manager = tokio::spawn(datastore::manager(
        datastore_receiver,
        connpool.clone(),
        Some(Duration::from_secs(config.read().sql_db_cleanup_seconds)),
    ));

    match goatns::cli::cli_commands(
        datastore_sender.clone(),
        &clap_results,
        &config.read().zone_file,
    )
    .await
    {
        Ok(resp) => {
            if resp == SystemState::Server {
                let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
                let udpserver = tokio::spawn(servers::udp_server(
                    config.read(),
                    datastore_sender.clone(),
                    agent_tx.clone(),
                ));
                let tcpserver = tokio::spawn(servers::tcp_server(
                    config.read(),
                    datastore_sender.clone(),
                    agent_tx.clone(),
                ));

                let apiserver =
                    goatns::web::build(datastore_sender.clone(), config.read(), connpool.clone())
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

fn main() -> Result<(), io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to start main thread!")
        .block_on(async { run().await })
}

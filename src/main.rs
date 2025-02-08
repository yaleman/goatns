use clap::Parser;
use goatns::cli::{add_admin_user, default_config, export_zone_file, import_zones, Cli, Commands};
use goatns::enums::SystemState;
use goatns::error::GoatNsError;
use goatns::utils::start_channels;
use sqlx::SqlitePool;
use std::io;
use std::time::Duration;
use tracing::{debug, error, info};

use goatns::config::{setup_logging, ConfigFile};
use goatns::datastore;
use goatns::db;
use goatns::servers;
use tokio::time::sleep;

async fn run() -> Result<(), GoatNsError> {
    // let clap_results = clap_parser();

    let cli = Cli::parse();

    let config = ConfigFile::try_as_cowcell(cli.config())
        .map_err(|err| GoatNsError::StartupError(format!("Config loading failed! {:?}", err)))?;

    let logger = setup_logging(config.read(), cli.debug())
        .await
        .map_err(|err| GoatNsError::StartupError(format!("Log setup failed! {:?}", err)))?;

    let config_result = ConfigFile::check_config(config.write().await).await;

    if let Commands::ConfigCheck { .. } = cli.command {
        info!(
            "{}",
            config
                .read()
                .as_json_pretty()
                .expect("Failed to serialize config!")
        );
        match config_result {
            Ok(_) => info!("Checking config... [OK!]"),
            Err(_) => error!("Checking config... [ERR!]"),
        };
    }

    // sometimes you just have to print some errors
    if let Err(errors) = config_result {
        for error in errors {
            error!("{error:}")
        }
        error!("Shutting down due to error!");
        logger.shutdown();
        return Err(GoatNsError::StartupError(
            "Config check failed!".to_string(),
        ));
    };

    if let Commands::ConfigCheck { .. } = cli.command {
        info!("Shutting down after config check.");
        logger.shutdown();
        return config_result.map_err(|err| {
            GoatNsError::StartupError(format!("Config check failed! Errors: {}", err.join(" ")))
        });
    };

    info!("Configuration: {}", *config.read());

    let (agent_tx, datastore_sender, datastore_receiver) = start_channels();

    // start up the DB
    let connpool: SqlitePool = db::get_conn(config.read())
        .await
        .map_err(|err| GoatNsError::StartupError(format!("DB Setup failed: {:?}", err)))?;

    db::start_db(&connpool).await?;

    // start all the things!

    // we only run the cron if the server is going to be running

    let cron_db_cleanup_timer = match cli.command {
        goatns::cli::Commands::Server { .. } => {
            Some(Duration::from_secs(config.read().sql_db_cleanup_seconds))
        }
        _ => None,
    };

    let datastore_manager = tokio::spawn(datastore::manager(
        datastore_receiver,
        connpool.clone(),
        cron_db_cleanup_timer,
    ));

    let next_step = match cli.command {
        Commands::Server { .. } => SystemState::Server,
        Commands::AddAdmin { .. } => {
            let _ = add_admin_user(datastore_sender.clone()).await;
            SystemState::ShuttingDown
        }
        Commands::ImportZones {
            sopt: _,
            filename,
            zone,
        } => {
            info!("Importing zones from {filename}");
            import_zones(datastore_sender.clone(), &filename, zone)
                .await
                .map_err(|e| GoatNsError::Generic(format!("Error importing {filename}: {e:?}")))?;

            SystemState::Import
        }
        Commands::ConfigCheck { .. } => SystemState::ShuttingDown,
        Commands::ExportConfig { .. } => {
            default_config();
            SystemState::ShuttingDown
        }
        Commands::ExportZone {
            sopt: _,
            zone_name,
            output_filename,
        } => {
            info!("Exporting zone {zone_name} to {output_filename}");
            let res =
                export_zone_file(datastore_sender.clone(), &zone_name, &output_filename).await;
            if let Err(err) = res {
                error!("{err}");
            }
            SystemState::Export
        }
    };

    if let SystemState::Server = next_step {
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
            goatns::web::build(datastore_sender.clone(), config.read(), connpool.clone()).await?;

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
    };
    debug!("Finishing up...");
    logger.shutdown();
    Ok(())
}

fn main() -> Result<(), io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to start main thread!")
        .block_on(async { run().await })
        .map_err(|err| {
            error!("{err:?}");
            err.into()
        })
}

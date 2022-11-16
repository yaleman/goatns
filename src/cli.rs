//! Code related to CLI things
//!

use clap::{arg, command, value_parser, Arg, ArgMatches};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;

use crate::config::ConfigFile;
use crate::datastore::Command;
use crate::enums::SystemState;
use crate::zones::FileZone;

/// Handles the command-line arguments.
pub fn clap_parser() -> ArgMatches {
    command!()
        .arg(
            arg!(
                -c --config <FILE> "Sets a custom config file"
            )
            .required(false)
            .value_parser(value_parser!(String)),
        )
        .arg(
            Arg::new("configcheck")
                .short('t')
                .long("configcheck")
                .help("Check the config file, show it and then quit.")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("export_config")
                .long("export-default-config")
                .help("Export a default config file.")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("export_zone")
                .short('e')
                .long("export-zone")
                .help("Export a single zone.")
                .value_parser(value_parser!(String)),
        )
        .arg(
            Arg::new("import_zones")
                .short('i')
                .long("import-zones")
                .help("Import a single zone file.")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("import_zone")
                .long("import-zone")
                .help("Import a single zone from a file.")
                .value_parser(value_parser!(String)),
        )
        .arg(
            Arg::new("filename")
                .short('f')
                .long("filename")
                .help("Filename to save to (used in other commands).")
                .value_parser(value_parser!(String)),
        )
        .arg(
            Arg::new("add_admin")
                .long("add-admin")
                .help("Add a new admin user.")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("use_zonefile")
                .long("using-zonefile")
                .help("Load the zone file into the DB on startup, typically used for testing.")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches()
}

/// Turns the clap inputs into actions.
pub async fn cli_commands(
    tx: mpsc::Sender<Command>,
    clap_results: &ArgMatches,
    zone_file: &Option<String>,
) -> Result<SystemState, String> {
    if clap_results.get_flag("add_admin") {
        let _ = add_admin_user(tx).await;
        return Ok(SystemState::ShuttingDown);
    }
    if clap_results.get_flag("export_config") {
        default_config();
        return Ok(SystemState::ShuttingDown);
    }

    // Load the specified zone file on startup
    if clap_results.get_flag("use_zonefile") {
        if let Some(zone_file) = zone_file {
            if let Err(error) = import_zones(tx.clone(), zone_file.to_owned(), None).await {
                log::error!("Failed to import zone file! {error:?}");
                return Ok(SystemState::ShuttingDown);
            }
        }
    }

    if let Some(zone_name) = clap_results.get_one::<String>("export_zone") {
        if let Some(output_filename) = clap_results.get_one::<String>("filename") {
            log::info!("Exporting zone {zone_name} to {output_filename}");
            let res = export_zone_file(tx, zone_name, output_filename).await;
            if let Err(err) = res {
                log::error!("{err}");
            }
            return Ok(SystemState::Export);
        } else {
            log::error!("You need to specify a a filename to save to.");
            return Ok(SystemState::ShuttingDown);
        }
    };

    if clap_results.get_flag("import_zones") {
        if let Some(filename) = clap_results.get_one::<String>("filename") {
            log::info!("Importing zones from {filename}");
            import_zones(tx, filename.to_owned(), None)
                .await
                .map_err(|e| format!("Error importing {filename}: {e:?}"))?;

            return Ok(SystemState::Import);
        } else {
            log::error!("You need to specify a a filename to save to.");
            return Ok(SystemState::ShuttingDown);
        }
    };
    if let Some(zone_name) = clap_results.get_one::<String>("import_zone") {
        if let Some(filename) = clap_results.get_one::<String>("filename") {
            log::info!("Importing zones from {filename}");
            import_zones(tx, filename.to_owned(), Some(zone_name.to_owned()))
                .await
                .map_err(|e| format!("Error importing {filename}: {e:?}"))?;

            return Ok(SystemState::Import);
        } else {
            log::error!("You need to specify a a filename to save to.");
            return Ok(SystemState::ShuttingDown);
        }
    };
    Ok(SystemState::Server)
}

/// Output a default configuration file, based on the [crate::config::ConfigFile] object.
pub fn default_config() {
    let output = match serde_json::to_string_pretty(&ConfigFile::default()) {
        Ok(value) => value,
        Err(_) => {
            log::error!("I don't know how, but we couldn't parse our own config file def.");
            "".to_string()
        }
    };
    println!("{output}");
}

/// Dump a zone to a file
pub async fn export_zone_file(
    tx: mpsc::Sender<Command>,
    zone_name: &String,
    filename: &String,
) -> Result<(), String> {
    // make a channel

    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: Command = Command::GetZone {
        id: None,
        name: Some(zone_name.clone()),
        resp: tx_oneshot,
    };
    if let Err(error) = tx.send(ds_req).await {
        return Err(format!(
            "failed to send to datastore from export_zone_file {error:?}"
        ));
    };

    let zone: Option<FileZone> = match rx_oneshot.await {
        Ok(value) => value,
        Err(err) => return Err(format!("rx from ds failed {err:?}")),
    };
    eprintln!("Got filezone: {zone:?}");

    if zone.is_none() {
        log::warn!("Couldn't find the zone {zone_name}");
        return Ok(());
    }

    // dump the zone
    let zone_bytes = serde_json::to_string_pretty(&zone.unwrap()).unwrap();

    // open the file
    let mut file = tokio::fs::File::create(filename)
        .await
        .map_err(|e| format!("Failed to open file {e:?}"))
        .unwrap();
    // write the thing
    file.write_all(zone_bytes.as_bytes())
        .await
        .map_err(|e| format!("Failed to write file: {e:?}"))
        .unwrap();
    // make some cake

    Ok(())
}

/// Import zones from a file
pub async fn import_zones(
    tx: mpsc::Sender<Command>,
    filename: String,
    zone_name: Option<String>,
) -> Result<(), String> {
    let (tx_oneshot, mut rx_oneshot) = oneshot::channel();
    let msg = Command::ImportFile {
        filename,
        resp: tx_oneshot,
        zone_name,
    };
    if let Err(err) = tx.send(msg).await {
        log::error!("Failed to send message to datastore: {err:?}");
    }
    loop {
        let res = rx_oneshot.try_recv();
        match res {
            Err(error) => {
                if let oneshot::error::TryRecvError::Closed = error {
                    break;
                }
            }
            Ok(()) => break,
        };
        sleep(std::time::Duration::from_micros(500)).await;
    }
    Ok(())
    // rx_oneshot.await.map_err(|e| format!("Failed to receive result: {e:?}"))
}

/// Presents the CLI UI to add an admin user.
pub async fn add_admin_user(tx: mpsc::Sender<Command>) -> Result<(), ()> {
    // prompt for the username
    println!("Creating admin user, please enter their username from the identity provider");
    let username: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Username")
        .interact_text()
        .unwrap();

    println!(
        "The authentication reference is the unique user identifier in the Identity Provider."
    );
    let authref: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Authentication Reference:")
        .interact_text()
        .unwrap();

    println!(
        r#"

Creating the following user:


Username: {username}
Authref:  {authref}

"#
    );
    // show the details and confirm them
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Do these details look correct?")
        .interact_opt();
    if confirm.is_err() || confirm.unwrap().is_none() {
        log::warn!("Cancelled user creation");
        return Err(());
    };

    // create oneshot
    let (tx_oneshot, rx_oneshot) = oneshot::channel();

    let new_user = Command::CreateUser {
        username: username.clone(),
        authref: authref.clone(),
        admin: true,
        disabled: false,
        resp: tx_oneshot,
    };
    // send command
    if let Err(error) = tx.send(new_user).await {
        log::error!("Failed to send new user command for username {username:?}: {error:?}");
        return Err(());
    };
    // wait for the response
    match rx_oneshot.await {
        Ok(res) => match res {
            true => {
                log::info!("Successfully created user!");
                Ok(())
            }
            false => {
                log::error!("Failed to create user! Check datastore logs.");
                Err(())
            }
        },
        Err(error) => {
            log::debug!("Failed to rx result from datastore: {error:?}");
            Err(())
        }
    }
}

//! Code related to CLI things
//!

use clap::*;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::ConfigFile;
use crate::datastore::Command;
use crate::zones::FileZone;

#[derive(Parser, Clone)]
pub struct SharedOpts {
    #[clap(short, long, help = "Configuration file")]
    config: Option<String>,
    #[clap(short, long)]
    debug: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    Server {
        #[clap(flatten)]
        sopt: SharedOpts,
    },
    AddAdmin {
        #[clap(flatten)]
        sopt: SharedOpts,
    },
    ImportZones {
        #[clap(flatten)]
        sopt: SharedOpts,
        filename: String,
        #[clap(short, long, help = "Specific Zone name to import")]
        zone: Option<String>,
    },
    ConfigCheck {
        #[clap(flatten)]
        sopt: SharedOpts,
    },
    ExportConfig {
        #[clap(flatten)]
        sopt: SharedOpts,
    },
    ExportZone {
        #[clap(flatten)]
        sopt: SharedOpts,
        zone_name: String,
        output_filename: String,
    },
}

impl Default for Commands {
    fn default() -> Self {
        Commands::Server {
            sopt: SharedOpts {
                config: None,
                debug: false,
            },
        }
    }
}

#[derive(Parser)]
#[command(arg_required_else_help(false))]
/// Yet another authoritative DNS name server. But with goat references.
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn config(&self) -> Option<String> {
        match &self.command {
            Commands::Server { sopt } => sopt.config.clone(),
            _ => None,
        }
    }

    pub fn debug(&self) -> bool {
        match &self.command {
            Commands::Server { sopt, .. } => sopt.debug,
            _ => false,
        }
    }
}

/// Output a default configuration file, based on the [crate::config::ConfigFile] object.
pub fn default_config() {
    let output = match serde_json::to_string_pretty(&ConfigFile::default()) {
        Ok(value) => value,
        Err(_) => {
            error!("I don't know how, but we couldn't parse our own config file def.");
            "".to_string()
        }
    };
    println!("{output}");
}

/// Dump a zone to a file
pub async fn export_zone_file(
    tx: mpsc::Sender<Command>,
    zone_name: &str,
    filename: &str,
) -> Result<(), String> {
    // make a channel

    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: Command = Command::GetZone {
        id: None,
        name: Some(zone_name.to_string()),
        resp: tx_oneshot,
    };
    if let Err(error) = tx.send(ds_req).await {
        return Err(format!(
            "failed to send to datastore from export_zone_file {error:?}"
        ));
    };
    debug!("Sent request to datastore");

    let zone: Option<FileZone> = match rx_oneshot.await {
        Ok(value) => value,
        Err(err) => return Err(format!("rx from ds failed {err:?}")),
    };
    eprintln!("Got filezone: {zone:?}");

    let zone_bytes = match zone {
        None => {
            warn!("Couldn't find the zone {zone_name}");
            return Ok(());
        }
        Some(zone) => serde_json::to_string_pretty(&zone).map_err(|err| {
            format!(
                "Failed to serialize zone {zone_name} to json: {err:?}",
                zone_name = zone_name,
                err = err
            )
        })?,
    };

    // open the file
    let mut file = tokio::fs::File::create(filename)
        .await
        .map_err(|e| format!("Failed to open file {e:?}"))?;
    // write the thing
    file.write_all(zone_bytes.as_bytes())
        .await
        .map_err(|e| format!("Failed to write file: {e:?}"))?;
    // make some cake

    Ok(())
}

/// Import zones from a file
pub async fn import_zones(
    tx: mpsc::Sender<Command>,
    filename: &str,
    zone_name: Option<String>,
) -> Result<(), String> {
    let (tx_oneshot, mut rx_oneshot) = oneshot::channel();
    let msg = Command::ImportFile {
        filename: filename.to_string(),
        resp: tx_oneshot,
        zone_name,
    };
    if let Err(err) = tx.send(msg).await {
        error!("Failed to send message to datastore: {err:?}");
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
        .map_err(|e| {
            error!("Failed to get username from user: {e:?}");
        })?;

    println!(
        "The authentication reference is the unique user identifier in the Identity Provider."
    );
    let authref: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Authentication Reference:")
        .interact_text()
        .map_err(|e| {
            error!("Failed to get auth reference from user: {e:?}");
        })?;

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

    match confirm {
        Ok(Some(true)) => {}
        Ok(Some(false)) | Ok(None) | Err(_) => {
            warn!("Cancelled user creation");
            return Err(());
        }
    }

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
        error!("Failed to send new user command for username {username:?}: {error:?}");
        return Err(());
    };
    // wait for the response
    match rx_oneshot.await {
        Ok(res) => match res {
            true => {
                info!("Successfully created user!");
                Ok(())
            }
            false => {
                error!("Failed to create user! Check datastore logs.");
                Err(())
            }
        },
        Err(error) => {
            debug!("Failed to rx result from datastore: {error:?}");
            Err(())
        }
    }
}

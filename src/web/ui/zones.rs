//! Zone-ui-things

use std::collections::HashMap;

use axum::extract::{OriginalUri, State};
use axum::response::Redirect;
use axum::Form;
use goat_lib::validators::dns_name;
use serde::Deserialize;
use tower_sessions::Session;
use tracing::{debug, error, info};

use crate::datastore::Command;
use crate::db::User;
use crate::web::ui::check_logged_in;
use crate::web::utils::Urls;
use crate::web::GoatState;
use crate::zones::FileZone;

#[derive(Deserialize, Debug)]
pub(crate) struct NewZoneForm {
    name: String,
}

pub(crate) async fn zones_new_post(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Form(form): Form<NewZoneForm>,
) -> Result<Redirect, Redirect> {
    debug!("Received new zone form: name={:?}", form.name);

    let user: User = check_logged_in(&mut session, path, state.clone()).await?;

    let userid = match user.id {
        Some(id) => id,
        None => {
            return Err(Urls::Home.redirect_with_query(HashMap::from([(
                "error".to_string(),
                "No user ID found".to_string(),
            )])));
        }
    };

    // validate the zone is valid
    if form.name.is_empty() {
        return Err(Urls::Home.redirect_with_query(HashMap::from([(
            "error".to_string(),
            "Zone name cannot be empty!".to_string(),
        )])));
    }
    // Validate that form.name is a valid DNS entry
    if !dns_name(&form.name) {
        return Err(Urls::Home.redirect_with_query(HashMap::from([(
            "error".to_string(),
            "Invalid DNS name".to_string(),
        )])));
    }

    // check if the zone already exists
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    debug!("Checking if {} exists", form.name);

    let getzonemsg = Command::GetZone {
        id: None,
        name: Some(form.name.clone()),
        resp: os_tx,
    };
    if let Err(err) = state.read().await.tx.send(getzonemsg).await {
        error!("Error sending message to datastore: {:?}", err);
        return Err(Urls::Home.redirect_with_query(HashMap::from([(
            "error".to_string(),
            "Error checking if zone exists... please try again.".to_string(),
        )])));
    };

    match os_rx.await {
        Ok(Some(_)) => {
            debug!("Zone already exists: {:?}", form.name);
            return Err(Urls::Home.redirect_with_query(HashMap::from([(
                "error".to_string(),
                "Zone already exists!".to_string(),
            )])));
        }
        Ok(None) => {
            debug!("Zone {} doesn't exist, we can continue", form.name);
        }
        Err(err) => {
            error!("Error getting zone {}: {:?}", form.name, err);
            return Err(Urls::Home.redirect_with_query(HashMap::from([(
                "error".to_string(),
                "Error checking if zone exists... please try again.".to_string(),
            )])));
        }
    };

    if user.email.is_empty() {
        return Err(Urls::Home.redirect_with_query(HashMap::from([(
            "error".to_string(),
            "No email associate with your account, please update your profile!".to_string(),
        )])));
    }

    let zone = FileZone {
        id: None,
        name: form.name.clone(),
        records: vec![],
        rname: user.email.replace("@", "."),
        serial: 0,
        refresh: Default::default(),
        retry: Default::default(),
        expire: Default::default(),
        minimum: Default::default(),
    };

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let msg = Command::CreateZone {
        zone,
        userid,
        resp: os_tx,
    };

    if let Err(err) = state.read().await.tx.send(msg).await {
        error!("Error sending message to datastore: {:?}", err);
        return Err(Urls::Home.redirect_with_query(HashMap::from([(
            "error".to_string(),
            "Error creating zone!".to_string(),
        )])));
    };

    match os_rx.await {
        Ok(zone) => {
            info!("Zone {} created successfully", form.name);
            if let Some(id) = zone.id {
                debug!("Redirecting to /ui/zones/{}", id);
                Ok(Redirect::to(&format!("/ui/zones/{id}")))
            } else {
                error!("Redirecting to /ui/zones because zone didn't have an ID?");
                Err(Urls::ZonesList.redirect())
            }
        }
        Err(err) => {
            error!("Error creating zone {}: {:?}", form.name, err);
            Err(Urls::Home.redirect_with_query(HashMap::from([(
                "error".to_string(),
                "Error creating zone!".to_string(),
            )])))
        }
    }
}

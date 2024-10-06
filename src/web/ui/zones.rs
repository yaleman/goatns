//! Zone-ui-things

use axum::extract::{OriginalUri, State};
use axum::response::{IntoResponse, Redirect};
use axum::Form;
use goat_lib::validators::dns_name;
use serde::Deserialize;
use tower_sessions::Session;
use tracing::debug;

use crate::datastore::Command;
use crate::db::User;
use crate::web::utils::redirect_to_login;
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
) -> impl IntoResponse {
    check_logged_in!(state, session, path);

    debug!("Received new zone form: name={:?}", form.name);

    let user: User = match session.get("user").await.unwrap() {
        Some(val) => {
            log::info!("current user: {val:?}");
            val
        }
        None => return redirect_to_login().into_response(),
    };

    // validate the zone is valid
    if form.name.is_empty() {
        return Redirect::to("/ui/zones?error=Zone name cannot be empty").into_response();
    }
    // Validate that form.name is a valid DNS entry
    if !dns_name(&form.name) {
        return Redirect::to("/ui/zones?error=Invalid DNS name").into_response();
    }

    // check if the zone already exists
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    log::debug!("Checking if {} exists", form.name);

    let getzonemsg = Command::GetZone {
        id: None,
        name: Some(form.name.clone()),
        resp: os_tx,
    };
    if let Err(err) = state.read().await.tx.send(getzonemsg).await {
        log::error!("Error sending message to datastore: {:?}", err);
        return Redirect::to("/ui/zones?error=Error checking if zone exists!").into_response();
    };

    match os_rx.await {
        Ok(Some(_)) => {
            log::debug!("Zone already exists: {:?}", form.name);
            return Redirect::to("/ui/zones?error=Zone already exists!").into_response();
        }
        Ok(None) => {
            log::debug!("Zone {} doesn't exist, we can continue", form.name);
        }
        Err(err) => {
            log::error!("Error getting zone {}: {:?}", form.name, err);
            return Redirect::to("/ui/zones?error=Error checking if zone exists!").into_response();
        }
    };

    if user.email.is_empty() {
        return Redirect::to("/ui/zones?error=No email address associated with user!")
            .into_response();
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
    let msg = Command::CreateZone { zone, resp: os_tx };

    if let Err(err) = state.read().await.tx.send(msg).await {
        log::error!("Error sending message to datastore: {:?}", err);
        return Redirect::to("/ui/zones?error=Error creating zone!").into_response();
    };

    match os_rx.await {
        Ok(zone) => {
            log::info!("Zone {} created successfully", form.name);
            if let Some(id) = zone.id {
                log::info!("Redirecting to /ui/zones/{}", id);
                Redirect::to(&format!("/ui/zones/{}", id)).into_response()
            } else {
                log::error!("Redirecting to /ui/zones because zone didn't have an ID?");
                Redirect::to("/ui/zones").into_response()
            }
        }
        Err(err) => {
            log::error!("Error creating zone {}: {:?}", form.name, err);
            Redirect::to("/ui/zones?error=Error creating zone!").into_response()
        }
    }
}

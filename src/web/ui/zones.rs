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

    let _user: User = match session.get("user").await.unwrap() {
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

    unimplemented!("haven't finished this yet!")
}

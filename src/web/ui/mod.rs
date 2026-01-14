use std::collections::HashMap;

use crate::datastore::Command;
use crate::db::entities;
use crate::web::api::filezone::ApiZoneResponse;
use crate::web::constants::{SESSION_REDIRECT_KEY, SESSION_USER_KEY};
use crate::web::utils::Urls;
use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use serde::Deserialize;
use tower_sessions::Session;
use tracing::{debug, error, instrument, trace};
use uuid::Uuid;

use super::GoatState;

mod admin_ui;
mod profile;
mod user_settings;
mod zones;

#[derive(Template, WebTemplate)]
#[template(path = "view_zones.html")]
pub(crate) struct TemplateViewZones {
    zones: Vec<entities::zones::Model>,
    pub user_is_admin: bool,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "view_zone.html")]
pub(crate) struct TemplateViewZone {
    zone: ApiZoneResponse,
    pub user_is_admin: bool,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ViewZonesQueryString {
    message: Option<String>,
    error: Option<String>,
}

#[instrument(level = "info", skip(state, session))]
pub(crate) async fn zones_list(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Query(query): Query<ViewZonesQueryString>,
) -> Result<TemplateViewZones, impl IntoResponse> {
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())?;
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let offset = 0;
    let limit = 20;

    trace!("Sending request for zones");
    if let Err(err) = state
        .read()
        .await
        .tx
        .send(Command::GetZoneNames {
            resp: os_tx,
            user_id: user.id,
            offset,
            limit,
        })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        error!("failed to send GetZoneNames command to datastore: {err:?}");
        return Err(Urls::Dashboard.redirect().into_response());
    };

    let zones = os_rx.await.map_err(|err| {
        error!("Failed to get zones for user={:?} error={:?}", user.id, err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error getting zones: {err:?}"),
        )
            .into_response()
    })?;

    Ok(TemplateViewZones {
        zones,
        user_is_admin: user.admin,
        message: query.message,
        error: query.error,
    })
}

#[instrument(level = "debug", skip(state))]
pub(crate) async fn zone_view(
    OriginalUri(path): OriginalUri,
    Path(id): Path<Uuid>,
    State(state): State<GoatState>,
    mut session: Session,
) -> Result<TemplateViewZone, impl IntoResponse> {
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())?;

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(id),
        name: None,
    };
    debug!("{cmd:?}");
    if let Err(err) = state.read().await.tx.send(cmd).await {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        error!("failed to send GetZone command to datastore: {err:?}");
        return Err(Urls::ZonesList.redirect().into_response());
    };

    let zone = match os_rx.await {
        Ok(zone) => match zone {
            Some(value) => value,
            None => {
                return Err((
                    axum::http::StatusCode::NOT_FOUND,
                    format!("Zone '{id}' not found"),
                )
                    .into_response());
            }
        },
        Err(err) => {
            error!("failed to get response from datastore: {err:?}");
            return Err(Urls::ZonesList.redirect().into_response());
        }
    };

    trace!("Returning zone: {zone:?}");
    Ok(TemplateViewZone {
        zone: zone.into(),
        user_is_admin: user.admin,
    })
}

pub async fn check_logged_in(
    session: &mut Session,
    path: Uri,
) -> Result<entities::users::Model, Redirect> {
    let user: Option<entities::users::Model> = session
        .get(SESSION_USER_KEY)
        .await
        .map_err(|_e| Urls::Login.redirect())?;

    let redirect_path = Some(
        path.path_and_query()
            .map(|v| v.to_string())
            .unwrap_or("/".to_string()),
    );

    // Check the database to make sure they're actually legit and not disabled
    match user {
        Some(user) => {
            if user.disabled {
                debug!("User {} is disabled, clearing session", user.id);
                session.clear().await;
                Err(Urls::Login.redirect())
            } else {
                Ok(user)
            }
        }
        None => {
            session.clear().await;

            session
                .insert(SESSION_REDIRECT_KEY, redirect_path)
                .await
                .map_err(|e| {
                    debug!("Couldn't store redirect for user: {e:?}");
                    Urls::Home.redirect_with_query(HashMap::from([(
                        "error",
                        "An error storing your session occurred!",
                    )]))
                })?;
            debug!("Not-logged-in-user tried to log in, how rude!");
            Err(Urls::Login.redirect())
        }
    }
}

#[derive(Template, WebTemplate)]
#[template(path = "dashboard.html")]
pub(crate) struct DashboardTemplate /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
}

pub(crate) async fn dashboard(
    // State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> Result<DashboardTemplate, Redirect> {
    let user = check_logged_in(&mut session, path).await?;

    Ok(DashboardTemplate {
        user_is_admin: user.admin,
    })
}

pub fn new() -> Router<GoatState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/zones/{id}", get(zone_view))
        .route("/zones/list", get(zones_list))
        .route("/zones/new", post(zones::zones_new_post))
        .route("/profile", get(profile::user_profile_get))
        .nest("/settings", user_settings::router())
        .nest("/admin", admin_ui::router())
}

use std::collections::HashMap;

use crate::datastore::Command;
use crate::db::{DBEntity, User};
use crate::web::utils::Urls;
use crate::zones::FileZone;
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use tower_sessions::Session;
use tracing::{debug, error, instrument, trace};

use super::GoatState;

mod admin_ui;
mod profile;
mod user_settings;
mod zones;

#[derive(Template, WebTemplate)]
#[template(path = "view_zones.html")]
pub(crate) struct TemplateViewZones {
    zones: Vec<FileZone>,
    pub user_is_admin: bool,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "view_zone.html")]
pub(crate) struct TemplateViewZone {
    zone: FileZone,
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
    let user = check_logged_in(&mut session, path, state.clone())
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
            user: user.clone(),
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
    Path(name_or_id): Path<i64>,
    State(state): State<GoatState>,
    mut session: Session,
) -> Result<TemplateViewZone, impl IntoResponse> {
    let user = check_logged_in(&mut session, path, state.clone())
        .await
        .map_err(|err| err.into_response())?;

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(name_or_id),
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
                    format!("Zone '{name_or_id}' not found"),
                )
                    .into_response())
            }
        },
        Err(err) => {
            error!("failed to get response from datastore: {err:?}");
            return Err(Urls::ZonesList.redirect().into_response());
        }
    };

    trace!("Returning zone: {zone:?}");
    Ok(TemplateViewZone {
        zone,
        user_is_admin: user.admin,
    })
}

pub async fn check_logged_in(session: &mut Session, path: Uri, state: GoatState) -> Result<User, Redirect> {
    let authref: Option<String> = session
        .get("authref")
        .await
        .map_err(|_e| Urls::Login.redirect())?;

    let redirect_path = Some(
        path.path_and_query()
            .map(|v| v.to_string())
            .unwrap_or("/".to_string()),
    );
    if authref.is_none() {
        session.clear().await;

        session
            .insert("redirect", redirect_path)
            .await
            .map_err(|e| {
                debug!("Couldn't store redirect for user: {e:?}");
                Urls::Home.redirect_with_query(HashMap::from([(
                    "error",
                    "An error storing your session occurred!",
                )]))
            })?;
        debug!("Not-logged-in-user tried to log in, how rude!");
        return Err(Urls::Login.redirect());
    }
    debug!("session ok!");

    let user: User = match session.get("user").await.unwrap_or(None) {
        Some(val) => val,
        None => return Err(Urls::Login.redirect()),
    };

    // Check the database to make sure they're actually legit and not disabled
    if let Some(user_id) = user.id {
        let pool = state.read().await.connpool.clone();
        match User::get(&pool, user_id).await {
            Ok(db_user) => {
                if db_user.disabled {
                    debug!("User {} is disabled, clearing session", user_id);
                    session.clear().await;
                    Err(Urls::Login.redirect())
                } else {
                    // Return the user from the database to ensure we have fresh data
                    Ok(*db_user)
                }
            }
            Err(err) => {
                debug!("Failed to validate user {} from database: {err:?}", user_id);
                session.clear().await;
                Err(Urls::Login.redirect())
            }
        }
    } else {
        debug!("User has no ID, clearing session");
        session.clear().await;
        Err(Urls::Login.redirect())
    }
}

#[derive(Template, WebTemplate)]
#[template(path = "dashboard.html")]
pub(crate) struct DashboardTemplate /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
}

pub(crate) async fn dashboard(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> Result<DashboardTemplate, Redirect> {
    let user = check_logged_in(&mut session, path, state).await?;

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

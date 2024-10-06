use crate::datastore::Command;
use crate::db::User;
use crate::web::utils::{redirect_to_dashboard, redirect_to_login, redirect_to_zones_list};
use crate::zones::FileZone;
use askama::Template;
use axum::debug_handler;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{Response, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use tower_sessions::Session;

use super::GoatState;

mod admin_ui;
mod profile;
mod user_settings;
mod zones;

#[derive(Template)]
#[template(path = "view_zones.html")]
struct TemplateViewZones {
    zones: Vec<FileZone>,
    pub user_is_admin: bool,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "view_zone.html")]
struct TemplateViewZone {
    zone: FileZone,
    pub user_is_admin: bool,
}

#[derive(Deserialize)]
pub struct ViewZonesQueryString {
    message: Option<String>,
    error: Option<String>,
}

// #[debug_handler]
pub async fn zones_list(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Query(query): Query<ViewZonesQueryString>,
) -> impl IntoResponse {
    // if let Err(e) = check_logged_in(&state, &mut session, path).await {
    //     return e.into_response();
    // }
    check_logged_in!(state, session, path);
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let offset = 0;
    let limit = 20;

    let user: User = match session.get("user").await.unwrap() {
        Some(val) => {
            log::info!("current user: {val:?}");
            val
        }
        None => return redirect_to_login().into_response(),
    };

    log::trace!("Sending request for zones");
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
        log::error!("failed to send GetZoneNames command to datastore: {err:?}");
        return redirect_to_dashboard().into_response();
    };

    let zones = os_rx.await.expect("Failed to get response: {res:?}");

    log::debug!("about to return zone list... found {} zones", zones.len());
    let context = TemplateViewZones {
        zones,
        user_is_admin: user.admin,
        message: query.message,
        error: query.error,
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

#[debug_handler]
pub async fn zone_view(
    OriginalUri(path): OriginalUri,
    Path(name_or_id): Path<i64>,
    State(state): State<GoatState>,
    mut session: Session,
) -> impl IntoResponse {
    let user = match check_logged_in(&mut session, path).await {
        Ok(val) => val,
        Err(err) => return err.into_response(),
    };

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(name_or_id),
        name: None,
    };
    log::debug!("{cmd:?}");
    if let Err(err) = state.read().await.tx.send(cmd).await {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        log::error!("failed to send GetZone command to datastore: {err:?}");
        return redirect_to_zones_list().into_response();
    };

    let zone = match os_rx.await {
        Ok(zone) => match zone {
            Some(value) => value,
            None => {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    format!("Zone '{}' not found", name_or_id),
                )
                    .into_response()
            }
        },
        Err(err) => {
            log::error!("failed to get response from datastore: {err:?}");
            return redirect_to_zones_list().into_response();
        }
    };

    log::trace!("Returning zone: {zone:?}");
    let context = TemplateViewZone {
        zone,
        user_is_admin: user.admin,
    };
    Response::new(context.render().unwrap()).into_response()
}

pub async fn check_logged_in(session: &mut Session, path: Uri) -> Result<User, Redirect> {
    let authref = session.get::<String>("authref").await.unwrap();

    let redirect_path = Some(path.path_and_query().unwrap().to_string());
    if authref.is_none() {
        session.clear().await;

        session
            .insert("redirect", redirect_path)
            .await
            .map_err(|e| log::debug!("Couldn't store redirect for user: {e:?}"))
            .unwrap();
        log::warn!("Not-logged-in-user tried to log in, how rude!");
        // TODO: this should redirect to the current page
        return Err(redirect_to_login());
    }
    log::debug!("session ok!");

    let user = match session.get("user").await.unwrap() {
        Some(val) => val,
        None => return Err(redirect_to_login()),
    };

    // TODO: check the database to make sure they're actually legit and not disabled and blah
    Ok(user)
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
}

// #[debug_handler]
pub async fn dashboard(mut session: Session, OriginalUri(path): OriginalUri) -> impl IntoResponse {
    let user = match check_logged_in(&mut session, path).await {
        Ok(val) => val,
        Err(err) => return err.into_response(),
    };

    let context = DashboardTemplate {
        user_is_admin: user.admin,
    };
    // Html::from()).into_response()
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

pub fn new() -> Router<GoatState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/zones/:id", get(zone_view))
        .route("/zones/list", get(zones_list))
        .route("/zones/new", post(zones::zones_new_post))
        .route("/profile", get(profile::user_profile_get))
        .nest("/settings", user_settings::router())
        .nest("/admin", admin_ui::router())
}

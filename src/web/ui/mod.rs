use crate::datastore::Command;
use crate::db::User;
use crate::web::utils::{redirect_to_dashboard, redirect_to_login, redirect_to_zones_list};
use crate::zones::FileZone;
use askama::Template;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{Response, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::{Form, Router};
use axum_macros::debug_handler;
use log::debug;
use regex::Regex;
use serde::Deserialize;
use tower_sessions::Session;

use super::GoatState;

mod admin_ui;
mod user_settings;

#[derive(Template)]
#[template(path = "view_zones.html")]
struct TemplateViewZones {
    zones: Vec<FileZone>,
    pub user_is_admin: bool,
}

#[derive(Template)]
#[template(path = "view_zone.html")]
struct TemplateViewZone {
    zone: FileZone,
    pub user_is_admin: bool,
}

macro_rules! check_logged_in {
    ( $state:tt, $session:tt, $path:tt ) => {
        if let Err(e) = check_logged_in(&mut $session, $path).await {
            return e.into_response();
        }
    };
}

#[derive(Template)]
#[template(path = "zone_create.html")]
struct TemplateCreateZones {
    user_is_admin: bool,
    zone: String,
    #[allow(dead_code)]
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct FormCreateZone {
    #[allow(dead_code)]
    zone: String,
    #[allow(dead_code)]
    csrftoken: Option<String>,
}

static VALID_ZONE_REGEX: &str = r"^[a-zA-Z0-9\-_]+\.[a-z]+$";

#[debug_handler]
pub async fn zones_create_post(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Form(zoneform): Form<FormCreateZone>,
) -> impl IntoResponse {
    check_logged_in!(state, session, path);
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    debug!("Zoneform: {:?}", zoneform);

    let user: User = match session.get("user").unwrap() {
        Some(val) => {
            log::info!("current user: {val:?}");
            val
        }
        None => return redirect_to_login().into_response(),
    };

    let message = if zoneform.zone.trim().is_empty() {
        "Zone name cannot be empty".to_string()
    } else {
        if Regex::new(VALID_ZONE_REGEX)
            .unwrap()
            .is_match(zoneform.zone.trim())
        {
            // zone name is valid, send off a request to create it
            let command = Command::PostZone {
                resp: os_tx,
                user: user.clone(),
                zone_name: zoneform.zone.trim().to_string(),
            };
            match state.read().await.tx.send(command).await {
                Err(err) => {
                    log::error!("Failed to send message to backend: {:?}", err);

                    "Failed to send message to backend, try again please.".to_string()
                }
                Ok(_) => {
                    let result: bool = os_rx.await.expect("Failed to get response");
                    if result {
                        "Zone created".to_string()
                    } else {
                        "Zone already exists".to_string()
                    }
                }
            }
        } else {
            "Zone name is invalid".to_string()
        }
    };

    let context = TemplateCreateZones {
        // zones,
        user_is_admin: user.admin,
        zone: zoneform.zone.trim().to_string(),
        message,
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

// #[debug_handler]
pub async fn zones_create_get(
    State(_state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())
        .unwrap();

    let context = TemplateCreateZones {
        // zones,
        user_is_admin: user.admin,
        zone: "".to_string(),
        message: "".to_string(),
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

// #[debug_handler]
pub async fn zones_list(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    // if let Err(e) = check_logged_in(&state, &mut session, path).await {
    //     return e.into_response();
    // }
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())
        .unwrap();
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let offset = 0;
    let limit = 20;

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
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

#[debug_handler]
pub async fn zone_view(
    Path(name_or_id): Path<i64>,
    axum::extract::State(state): axum::extract::State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())
        .unwrap();

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

    let zone = match os_rx.await.expect("Failed to get response: {res:?}") {
        Some(value) => value,
        None => todo!("Send a not found"),
    };

    log::trace!("Returning zone: {zone:?}");
    let context = TemplateViewZone {
        zone,
        user_is_admin: user.admin,
    };
    Response::new(context.render().unwrap()).into_response()
}

pub async fn check_logged_in(session: &mut Session, path: Uri) -> Result<User, Redirect> {
    let authref = session.get::<String>("authref").unwrap();

    let redirect_path = Some(path.path_and_query().unwrap().to_string());
    if authref.is_none() {
        session.clear();

        session
            .insert("redirect", redirect_path)
            .map_err(|e| log::debug!("Couldn't store redirect for user: {e:?}"))
            .unwrap();
        log::warn!("Not-logged-in-user tried to log in, how rude!");
        // TODO: this should redirect to the current page
        return Err(redirect_to_login());
    }
    log::debug!("session ok!");

    let user = match session.get("user").unwrap() {
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
        .route(
            "/zones/create",
            get(zones_create_get).post(zones_create_post),
        )
        .nest("/settings", user_settings::router())
        .nest("/admin", admin_ui::router())
}

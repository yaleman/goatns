use crate::datastore::Command;
use crate::db::User;
use crate::web::utils::{redirect_to_dashboard, redirect_to_login, redirect_to_zones_list};
use crate::zones::FileZone;
use askama::Template;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{Response, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use axum_macros::debug_handler;
// use axum_macros::debug_handler;
use axum_sessions::extractors::WritableSession;

use super::GoatState;

mod user_settings;

#[derive(Template)]
#[template(path = "view_zones.html")]
struct TemplateViewZones {
    zones: Vec<FileZone>,
}

#[derive(Template)]
#[template(path = "view_zone.html")]
struct TemplateViewZone {
    zone: FileZone,
}

macro_rules! check_logged_in {
    ( $state:tt, $session:tt, $path:tt ) => {
        if let Err(e) = check_logged_in(&mut $session, $path).await {
            return e.into_response();
        }
    };
}

// #[debug_handler]
pub async fn zones_list(
    State(state): State<GoatState>,
    mut session: WritableSession,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    // if let Err(e) = check_logged_in(&state, &mut session, path).await {
    //     return e.into_response();
    // }
    check_logged_in!(state, session, path);
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let offset = 0;
    let limit = 20;

    let user: User = match session.get("user") {
        Some(val) => {
            log::info!("current user: {val:?}");
            val
        }
        None => return redirect_to_login().into_response(),
    };

    println!("Sending request for zones");
    if let Err(err) = state
        .read()
        .await
        .tx
        .send(Command::GetZoneNames {
            resp: os_tx,
            user,
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
    let context = TemplateViewZones { zones };
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
    mut session: WritableSession,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    if let Err(e) = check_logged_in(&mut session, path).await {
        return e.into_response();
    }
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
    let context = TemplateViewZone { zone };
    Response::new(context.render().unwrap()).into_response()
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate /*<'a>*/ {
    // name: &'a str,
}

pub async fn check_logged_in(session: &mut WritableSession, path: Uri) -> Result<(), Redirect> {
    let authref = session.get::<String>("authref");

    let redirect_path = Some(path.path_and_query().unwrap().to_string());
    if authref.is_none() {
        session.regenerate();
        session
            .insert("redirect", redirect_path)
            .map_err(|e| log::debug!("Couldn't store redirect for user: {e:?}"))
            .unwrap();
        log::warn!("Not-logged-in-user tried to log in, how rude!");
        // TODO: this should redirect to the current page
        return Err(redirect_to_login());
    }
    log::debug!("session ok!");
    // TODO: check the database to make sure they're actually legit and not disabled and blah
    Ok(())
}

// #[debug_handler]
pub async fn dashboard(
    mut session: WritableSession,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    if let Err(e) = check_logged_in(&mut session, path).await {
        return e.into_response();
    }

    let context = DashboardTemplate {};
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
        .nest("/settings", user_settings::router())
}

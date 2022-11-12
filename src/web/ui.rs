use crate::datastore::Command;
use crate::web::utils::{redirect_to_dashboard, redirect_to_zones_list, redirect_to_login};
use crate::zones::FileZone;
use askama::Template;
use axum::extract::{Path, OriginalUri};
use axum::http::{Response, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::{Extension, Router};
use axum_login::axum_sessions::extractors::WritableSession;
use axum_macros::debug_handler;

use super::SharedState;

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

#[debug_handler]
pub async fn zones_list(
    Extension(state): Extension<SharedState>,session: WritableSession, OriginalUri(path): OriginalUri
) -> impl IntoResponse {

    if let Err(e) = check_logged_in(&state, session, path).await {
        return e.into_response();
    }
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let state_writer = state.write().await;
    if let Err(err) = state_writer
        .tx
        .send(Command::GetZoneNames { resp: os_tx })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        log::error!("failed to send GetZoneNames command to datastore: {err:?}");
        return redirect_to_dashboard().into_response();
    };
    drop(state_writer);

    let zones = os_rx.await.expect("Failed to get response: {res:?}");

    log::debug!("about to return zone list...");
    let context = TemplateViewZones { zones };
    Response::builder().status(200).body(context.render().unwrap()).unwrap().into_response()
}

pub async fn zone_view(
    Path(name_or_id): Path<i64>,
    Extension(state): Extension<SharedState>,
    session: WritableSession,
    OriginalUri(path): OriginalUri
) -> impl IntoResponse {

    if let Err(e) = check_logged_in(&state, session, path).await {
        return e.into_response();
    }
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(name_or_id),
        name: None,
    };
    log::debug!("{cmd:?}");
    let state_writer = state.write().await;
    if let Err(err) = state_writer.tx.send(cmd).await {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        log::error!("failed to send GetZone command to datastore: {err:?}");
        return redirect_to_zones_list().into_response();
    };
    drop(state_writer);

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

pub async fn check_logged_in(_state: &SharedState, mut session: WritableSession, path: Uri) -> Result<(),Redirect> {
    let authref = session.get::<String>("authref");

    let redirect_path = Some(path.path_and_query().unwrap().to_string());
    if authref.is_none() {
        session.regenerate();
        session.insert("redirect", redirect_path).map_err(|e| log::debug!("Couldn't store redirect for user: {e:?}")).unwrap();
        log::warn!("Not-logged-in-user tried to log in, how rude!");
        // TODO: this should redirect to the current page
        return Err(redirect_to_login())
    }
    log::debug!("session ok!");
    // TODO: check the database to make sure they're actually legit and not disabled and blah
    Ok(())

}

#[debug_handler]
pub async fn dashboard(Extension(state): Extension<SharedState>, session: WritableSession,OriginalUri(path): OriginalUri) -> impl IntoResponse {
    if let Err(e) = check_logged_in(&state, session, path).await {
        return e.into_response();
    }

    let context = DashboardTemplate {};
    // Html::from()).into_response()
    Response::builder().status(200).body(context.render().unwrap()).unwrap().into_response()
}

pub fn new() -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/zones/:id", get(zone_view))
        .route("/zones/list", get(zones_list))
}

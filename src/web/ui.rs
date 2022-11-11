use crate::datastore::Command;
use crate::zones::FileZone;
use askama::Template;
use axum::extract::Path;
use axum::response::Html;
use axum::routing::get;
use axum::{Extension, Router};
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
    Extension(state): Extension<SharedState>,
) -> Result<Html<String>, &'static str> {
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let state_writer = state.write().await;
    if let Err(err) = state_writer
        .tx
        .send(Command::GetZoneNames { resp: os_tx })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        log::error!("failed to send GetZoneNames command to datastore: {err:?}");
        return Err("Failed to send request to backend");
    };
    drop(state_writer);

    let zones = os_rx.await.expect("Failed to get response: {res:?}");

    let context = TemplateViewZones { zones };
    Ok(Html::from(context.render().unwrap()))
}

pub async fn zone_view(
    Path(name_or_id): Path<i64>,
    Extension(state): Extension<SharedState>,
) -> Result<Html<String>, &'static str> {
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
        return Err("Failed to send request to backend");
    };
    drop(state_writer);

    let zone = match os_rx.await.expect("Failed to get response: {res:?}") {
        Some(value) => value,
        None => todo!("Send a not found"),
    };

    log::trace!("Returning zone: {zone:?}");
    let context = TemplateViewZone { zone };
    Ok(Html::from(context.render().unwrap()))
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate /*<'a>*/ {
    // name: &'a str,
}

pub async fn dashboard() -> Result<Html<String>, ()> {
    let context = DashboardTemplate {};
    Ok(Html::from(context.render().unwrap()))
}

pub fn new() -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/zones/:id", get(zone_view))
        .route("/zones/list", get(zones_list))
}

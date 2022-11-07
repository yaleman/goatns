use std::sync::Arc;

use askama::Template;
use axum::Extension;
use axum::extract::Path;
use axum::response::Html;



use crate::datastore::Command;
use crate::zones::FileZone;

use super::SharedState;

#[derive(Template)]
#[template(path = "index.html")]
struct HelloTemplate /*<'a>*/ {
    // name: &'a str,
}

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

pub async fn zones_list(Extension(req): Extension<Arc<SharedState>>) -> Result<Html<String>, &'static str> {
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    if let Err(err) = req
        .tx
        .send(Command::GetZoneNames { resp: os_tx })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        log::error!("failed to send GetZoneNames command to datastore: {err:?}");
        return Err("Failed to send request to backend");
    };

    let zones = os_rx.await.expect("Failed to get response: {res:?}");

    let context = TemplateViewZones { zones };
    Ok(Html::from(context.render().unwrap()))
}

pub async fn index() -> Result<Html<String>,()> {
    let context = HelloTemplate {};
    Ok(Html::from(context.render().unwrap()))
}

pub async fn zone_view(Path(name_or_id): Path<i64>, Extension(state): Extension<Arc<SharedState>>) -> Result<Html<String>,&'static str> {


    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    // TODO: fix this one
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(name_or_id),
        name: None,
    };
    log::debug!("{cmd:?}");
    if let Err(err) = state.tx
        .send(cmd)
        .await
    {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        log::error!("failed to send GetZone command to datastore: {err:?}");
        return Err("Failed to send request to backend");
    };

    let zone = match os_rx.await.expect("Failed to get response: {res:?}") {
        Some(value) => value,
        None => todo!("Send a not found"),
    };

    log::trace!("Returning zone: {zone:?}");
    let context = TemplateViewZone { zone };
    Ok(Html::from(context.render().unwrap()))
}

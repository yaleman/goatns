use askama::Template; // bring trait in scope
use tide::http::mime;
use tide::Response;

use crate::datastore::Command;
use crate::zones::FileZone;

use super::State;

#[derive(Template)]
#[template(path = "index.html")]
struct HelloTemplate/*<'a>*/ {
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


pub async fn zones_list(req: tide::Request<State>) -> tide::Result {
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    if let Err(err) = req
        .state()
        .tx
        .send(Command::GetZoneNames { resp: os_tx })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        log::error!("failed to send GetZoneNames command to datastore: {err:?}");
        return Err(tide::Error::from_str(
            tide::StatusCode::InternalServerError,
            "Failed to send request to backend".to_string(),
        ));
    };

    let zones = os_rx.await.expect("Failed to get response: {res:?}");

    eprintln!("{zones:?}");
    let context = TemplateViewZones { zones };
    tide_result_html!(context, 200)
}


pub async fn index(_req: tide::Request<State>) -> tide::Result {
    let hello = HelloTemplate {};
    tide_result_html!(hello, 200)
}

pub async fn zone_view(req: tide::Request<State>) -> tide::Result {
    let name_or_id = req.param("id")?;

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    if let Err(err) = req
        .state()
        .tx
        .send(Command::GetZone {
            resp: os_tx,
            name: name_or_id.to_string(),
        })
        .await
    {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        log::error!("failed to send GetZone command to datastore: {err:?}");
        return Err(tide::Error::from_str(
            tide::StatusCode::InternalServerError,
            "Failed to send request to backend".to_string(),
        ));
    };

    let zone = match os_rx.await.expect("Failed to get response: {res:?}") {
        Some(value) => value,
        None => todo!("Send a not found"),
    };

    log::trace!("Returning zone: {zone:?}");
    let context = TemplateViewZone { zone };
    tide_result_html!(context, 200)
}

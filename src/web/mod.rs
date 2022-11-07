use std::sync::Arc;

use crate::config::ConfigFile;
use crate::datastore;
use crate::enums::RecordType;
use crate::resourcerecord::InternalResourceRecord;
use axum::extract::MatchedPath;
use axum::routing::get;
use axum::{Extension, Json, Router};
use axum_extra::routing::SpaRouter;
use sqlx::{Pool, Sqlite};

use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

#[macro_use]
pub mod macros;

pub mod api;
pub mod generic;
pub mod ui;

pub const STATUS_OK: &str = "Ok";

async fn api_query(
    qname: MatchedPath,
    qtype: MatchedPath,
    state: Extension<Arc<SharedState>>,
) -> Result<Json<Vec<InternalResourceRecord>>, &'static str> {
    let rrtype: RecordType = qtype.as_str().into();
    if let RecordType::InvalidType = rrtype {
        // return Err(tide::BadRequest(
        // ));
        return Err("Invalid RRTYPE requested: {qtype:?}");
    }

    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: datastore::Command = datastore::Command::GetRecord {
        name: qname.as_str().into(),
        rrtype,
        rclass: crate::RecordClass::Internet,
        resp: tx_oneshot,
    };

    // here we talk to the datastore to pull the result
    // TODO: shared state req
    match state.tx.send(ds_req).await {
        Ok(_) => log::trace!("Sent a request to the datastore!"),
        // TODO: handle this properly
        Err(error) => log::error!("Error sending to datastore: {:?}", error),
    };

    let record: Option<crate::zones::ZoneRecord> = match rx_oneshot.await {
        Ok(value) => match value {
            Some(zr) => {
                log::debug!("DS Response: {}", zr);
                Some(zr)
            }
            None => {
                log::debug!("No response from datastore");
                return Err("No response from datastore");
            }
        },
        Err(error) => {
            log::error!("Failed to get response from datastore: {:?}", error);
            return Err("Sorry, something went wrong.");
        }
    };

    match record {
        None => Err(""), // TODO: 404
        Some(value) => Ok(Json::from(value.typerecords)),
    }
}

/// Internal State handler for the datastore object within the API
#[derive(Debug, Clone)]
pub struct SharedState {
    tx: Sender<datastore::Command>,
    // TODO: ensure we actually need to use the connpool, lulz
    #[allow(dead_code)]
    connpool: Pool<Sqlite>,
    config: ConfigFile,
}

pub async fn build(
    tx: Sender<datastore::Command>,
    config: ConfigFile,
    connpool: Pool<Sqlite>,
) -> axum::Router {
    // from https://docs.rs/axum/0.5.17/axum/index.html#using-request-extensions
    let shared_state = Arc::new(SharedState {
        tx,
        connpool,
        config,
    });

    Router::new()
        .route("/", get(generic::index))
        .route("/status", get(generic::status))
        .route("/query/:qname/:qtype", get(api_query))
        .merge(SpaRouter::new(
            "/ui/static",
            shared_state.config.api_static_dir.clone(),
        ))
        .nest("/ui", ui::new(shared_state.clone()))
        .nest("/api", api::new(shared_state))
}

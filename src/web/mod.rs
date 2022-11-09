/// # Web things
///
/// Uses axum/tower for protocol, askama for templating, confusion for the rest.
///
/// Example using shared state: https://github.com/tokio-rs/axum/blob/axum-v0.5.17/examples/key-value-store/src/main.rs
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::ConfigFile;
use crate::datastore;
// use axum::error_handling::HandleErrorLayer;
// use crate::enums::RecordType;
// use crate::resourcerecord::InternalResourceRecord;
// use axum::extract::MatchedPath;
use axum::routing::get;
use axum::{Extension, Router};
// use axum::{Extension, Json, Router};
use axum_extra::routing::SpaRouter;
use chrono::{DateTime, NaiveDateTime, Utc};
use kanidm_proto::oauth2::OidcDiscoveryResponse;
use sqlx::{Pool, Sqlite};

use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
// use tokio::sync::oneshot;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Level;

#[macro_use]
pub mod macros;

pub mod api;
pub mod auth;
pub mod generic;
pub mod ui;

pub const STATUS_OK: &str = "Ok";

// // TODO: look at the ServiceBuilder layers bits here: https://github.com/tokio-rs/axum/blob/dea36db400f27c025b646e5720b9a6784ea4db6e/examples/key-value-store/src/main.rs

type SharedState = Arc<RwLock<State>>;

#[derive(Debug)]
/// Internal State handler for the datastore object within the API
pub struct State {
    pub tx: Sender<datastore::Command>,
    // TODO: ensure we actually need to use the connpool in the web api shared state
    #[allow(dead_code)]
    pub connpool: Pool<Sqlite>,
    // TODO: ensure we actually need to use the config in the web api shared state
    #[allow(dead_code)]
    pub config: ConfigFile,
    pub oidc_config_updated: Option<DateTime<Utc>>,
    pub oidc_config: Option<Arc<OidcDiscoveryResponse>>,
}

// async fn api_query(
//     qname: MatchedPath,
//     qtype: MatchedPath,
//     state: Extension<Arc<SharedState>>,
// ) -> Result<Json<Vec<InternalResourceRecord>>, &'static str> {
//     let rrtype: RecordType = qtype.as_str().into();
//     if let RecordType::InvalidType = rrtype {
//         // return Err(tide::BadRequest(
//         // ));
//         return Err("Invalid RRTYPE requested: {qtype:?}");
//     }

//     let (tx_oneshot, rx_oneshot) = oneshot::channel();
//     let ds_req: datastore::Command = datastore::Command::GetRecord {
//         name: qname.as_str().into(),
//         rrtype,
//         rclass: crate::RecordClass::Internet,
//         resp: tx_oneshot,
//     };

//     // here we talk to the datastore to pull the result
//     // TODO: shared state req
//     match state.tx.send(ds_req).await {
//         Ok(_) => log::trace!("Sent a request to the datastore!"),
//         // TODO: handle this properly
//         Err(error) => log::error!("Error sending to datastore: {:?}", error),
//     };

//     let record: Option<crate::zones::ZoneRecord> = match rx_oneshot.await {
//         Ok(value) => match value {
//             Some(zr) => {
//                 log::debug!("DS Response: {}", zr);
//                 Some(zr)
//             }
//             None => {
//                 log::debug!("No response from datastore");
//                 return Err("No response from datastore");
//             }
//         },
//         Err(error) => {
//             log::error!("Failed to get response from datastore: {:?}", error);
//             return Err("Sorry, something went wrong.");
//         }
//     };

//     match record {
//         None => Err(""), // TODO: throw a 404 when we can't find a record
//         Some(value) => Ok(Json::from(value.typerecords)),
//     }
// }

pub async fn build(
    tx: Sender<datastore::Command>,
    config: ConfigFile,
    connpool: Pool<Sqlite>,
) -> axum::Router {
    let config_dir: PathBuf = shellexpand::tilde(&config.api_static_dir)
        .to_string()
        .into();
    // check to see if we can find the static dir things
    match config_dir.try_exists() {
        Ok(res) => match res {
            true => log::info!("Found static resources dir ({config_dir:#?}) for web API."),
            false => {
                log::error!("Couldn't find static resources dir ({config_dir:#?}) for web API!")
            }
        },
        Err(err) => match err.kind() {
            std::io::ErrorKind::PermissionDenied => {
                log::error!(
                    "Permission denied accssing static resources dir ({:?}) for web API: {}",
                    &config.api_static_dir,
                    err.to_string()
                )
            }
            std::io::ErrorKind::NotFound => {
                log::error!(
                    "Static resources dir ({:?}) not found for web API: {}",
                    &config.api_static_dir,
                    err.to_string()
                )
            }
            _ => log::error!(
                "Error accessing static resources dir ({:?}) for web API: {}",
                &config.api_static_dir,
                err.to_string()
            ),
        },
    }

    let static_router = SpaRouter::new("/ui/static", &config_dir);

    let oidc_config = match auth::oidc_discover(&config).await {
        Ok(val) => Some(Arc::new(val)),
        Err(error) => {
            log::error!("Failed to pull OIDC Discovery data: {error:?}");
            None
        }
    };
    let oidc_config_updated: Option<DateTime<Utc>> = match oidc_config.is_some() {
        false => None,
        true => Some(DateTime::from_utc(NaiveDateTime::default(), Utc)),
    };

    let state: SharedState = Arc::new(RwLock::new(State {
        tx,
        connpool,
        config,
        oidc_config_updated,
        oidc_config,
    }));

    // add u sum layerz https://docs.rs/tower-http/latest/tower_http/index.html
    let trace_layer =
        TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().level(Level::INFO));

    Router::new()
        .route("/", get(generic::index))
        .route("/status", get(generic::status))
        .merge(static_router)
        .nest("/ui", ui::new())
        .nest("/api", api::new())
        .nest("/auth", auth::new())
        .layer(
            ServiceBuilder::new()
                // Handle errors from middleware
                // .layer(HandleErrorLayer::new(handle_error))
                // .load_shed()
                // .concurrency_limit(1024)
                // .timeout(Duration::from_secs(10))
                .layer(TraceLayer::new_for_http())
                .layer(Extension(state))
                .layer(trace_layer)
                .into_inner(),
        )
}

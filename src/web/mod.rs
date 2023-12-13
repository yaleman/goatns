//! # Web things
//!
//! Uses axum/tower for protocol, askama for templating, confusion for the rest.
//!

#![allow(clippy::clone_on_copy)]
// ^ this is because the datetime in the goatchildState is a jerk
use crate::config::ConfigFile;
use crate::datastore;
use crate::web::middleware::csp;
use async_trait::async_trait;
use axum::error_handling::HandleErrorLayer;
use axum::extract::FromRef;
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::BoxError;
use axum::Router;
use axum_csp::CspUrlMatcher;
#[cfg(feature = "otel")]
#[cfg(not(test))]
use axum_tracing_opentelemetry::opentelemetry_tracing_layer;
use chrono::{DateTime, NaiveDateTime, Utc};
use concread::cowcell::asynch::CowCellReadTxn;
use log::error;
use oauth2::{ClientId, ClientSecret};
use openidconnect::Nonce;
use regex::RegexSet;
use sqlx::{Pool, Sqlite, SqlitePool};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
// use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use utils::handler_404;

use self::auth::CustomProviderMetadata;

#[macro_use]
pub mod macros;

pub mod api;
pub mod auth;
pub mod doh;
pub mod generic;
pub mod middleware;
pub mod ui;
pub mod utils;

pub const STATUS_OK: &str = "Ok";

// TODO: look at the ServiceBuilder layers bits here: https://github.com/tokio-rs/axum/blob/dea36db400f27c025b646e5720b9a6784ea4db6e/examples/key-value-store/src/main.rs

// type GoatState = Arc<RwLock<State>>;

#[async_trait]
pub trait GoatStateTrait {
    async fn connpool(&self) -> Pool<Sqlite>;
    async fn oidc_update<'life0>(&'life0 mut self, response: CustomProviderMetadata);
    async fn pop_verifier<'life0>(&'life0 mut self, csrftoken: String) -> Option<(String, Nonce)>;
    async fn oauth2_client_id(&self) -> ClientId;
    async fn oauth2_secret(&self) -> Option<ClientSecret>;
    async fn push_verifier(&mut self, csrftoken: String, verifier: (String, Nonce));
}

#[async_trait]
impl GoatStateTrait for GoatState {
    /// Get an sqlite connection pool
    async fn connpool(&self) -> Pool<Sqlite> {
        self.read().await.connpool.clone()
    }
    async fn oidc_update<'life0>(&'life0 mut self, response: CustomProviderMetadata) {
        log::debug!("Storing OIDC config!");
        let mut writer = self.write().await;
        writer.oidc_config = Some(response.clone());
        writer.oidc_config_updated =
            chrono::TimeZone::from_utc_datetime(&Utc, &NaiveDateTime::default());
        drop(writer);
    }

    async fn pop_verifier<'life0>(&'life0 mut self, csrftoken: String) -> Option<(String, Nonce)> {
        let mut writer = self.write().await;
        writer.oidc_verifier.remove(&csrftoken)
    }
    async fn oauth2_client_id(&self) -> ClientId {
        let client_id = self.read().await.config.oauth2_client_id.clone();
        ClientId::new(client_id)
    }

    async fn oauth2_secret(&self) -> Option<ClientSecret> {
        let client_secret = self.read().await.config.oauth2_secret.clone();
        Some(ClientSecret::new(client_secret))
    }

    /// Store the PKCE verifier details server-side for when the user comes back with their auth token
    async fn push_verifier(&mut self, csrftoken: String, verifier: (String, Nonce)) {
        let mut writer = self.write().await;
        log::trace!("Pushing CSRF token into shared state: token={csrftoken}");
        writer.oidc_verifier.insert(csrftoken, verifier);
    }
}

type GoatState = Arc<RwLock<GoatChildState>>;

#[derive(Clone, FromRef)]
/// Internal State handler for the datastore object within the API
pub struct GoatChildState {
    pub tx: Sender<datastore::Command>,
    pub connpool: SqlitePool,
    pub config: ConfigFile,
    pub oidc_config_updated: DateTime<Utc>,
    pub oidc_config: Option<auth::CustomProviderMetadata>,
    pub oidc_verifier: std::collections::HashMap<String, (String, Nonce)>,
    pub csp_matchers: Vec<CspUrlMatcher>,
}

fn check_static_dir_exists(static_dir: &PathBuf, config: &ConfigFile) -> bool {
    match static_dir.try_exists() {
        Ok(res) => match res {
            true => {
                log::info!("Found static resources dir ({static_dir:#?}) for web API.");
                return true;
            }
            false => {
                log::error!("Couldn't find static resources dir ({static_dir:#?}) for web API!")
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
    false
}

pub async fn build(
    tx: Sender<datastore::Command>,
    config: CowCellReadTxn<ConfigFile>,
    connpool: SqlitePool,
) -> Option<JoinHandle<Result<(), std::io::Error>>> {
    let static_dir: PathBuf = shellexpand::tilde(&config.api_static_dir)
        .to_string()
        .into();
    #[cfg(feature = "otel")]
    if let Err(error) = axum_tracing_opentelemetry::tracing_subscriber_ext::init_subscribers() {
        eprintln!("Failed to initialize OpenTelemetry tracing: {error:?}");
    };

    let session_layer = auth::build_auth_stores(config.clone(), connpool.clone()).await;

    let csp_matchers = vec![CspUrlMatcher::default_self(
        RegexSet::new([r"^(/|/ui)"]).unwrap(),
    )];

    // we set this to an hour ago so it forces update on startup
    let oidc_config_updated = Utc::now() - chrono::Duration::seconds(3600);
    // let config_clone: ConfigFile = ConfigFile::from(&config);
    let state = Arc::new(RwLock::new(GoatChildState {
        tx,
        connpool,
        config: (*config).clone(),
        oidc_config_updated,
        oidc_config: None,
        oidc_verifier: HashMap::new(),
        csp_matchers,
    }));

    let service_layer = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|err: BoxError| async move {
            error!("Something broke: {:?}", err);
            StatusCode::BAD_REQUEST
        }))
        .layer(session_layer)
        .layer(from_fn_with_state(state.clone(), csp::cspheaders));

    let router = Router::new()
        .route("/", get(generic::index))
        .nest("/ui", ui::new())
        .nest("/api", api::new())
        .nest("/auth", auth::new())
        .nest("/dns-query", doh::new())
        .with_state(state)
        .layer(service_layer);

    // here we add the tracing layer
    #[cfg(feature = "otel")]
    #[cfg(not(test))]
    let router = router.layer(opentelemetry_tracing_layer());

    let router = router.route("/status", get(generic::status));

    let router = match check_static_dir_exists(&static_dir, &config) {
        true => router.nest_service("/static", ServeDir::new(&static_dir)),
        false => router,
    };
    let router = router.layer(CompressionLayer::new()).fallback(handler_404);

    let tls_config = match config.get_tls_config().await {
        Ok(val) => val,
        Err(err) => {
            error!("{}", err);
            return None;
        }
    };

    let res: Option<JoinHandle<Result<(), std::io::Error>>> = Some(tokio::spawn(
        axum_server::bind_rustls(config.api_listener_address(), tls_config)
            .serve(router.into_make_service()),
    ));
    #[cfg(test)]
    println!(
        "Started Web server on https://{}",
        config.api_listener_address()
    );
    log::info!(
        "Started Web server on https://{}",
        config.api_listener_address()
    );
    res
}

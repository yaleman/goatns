//! # Web things
//!
//! Uses axum/tower for protocol, askama for templating, confusion for the rest.
//!
use crate::config::ConfigFile;
use crate::datastore;
use crate::web::middleware::csp;
use async_trait::async_trait;
use axum::handler::Handler;
use axum::http::StatusCode;
use axum::middleware::from_fn;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use axum_csp::CspUrlMatcher;
use axum_extra::routing::SpaRouter;
use axum_server::tls_rustls::RustlsConfig;
use chrono::{DateTime, NaiveDateTime, Utc};
use concread::cowcell::asynch::CowCellReadTxn;
use oauth2::{ClientId, ClientSecret, PkceCodeVerifier};
use openidconnect::Nonce;
use regex::RegexSet;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Level;

use self::auth::CustomProviderMetadata;

#[macro_use]
pub mod macros;

pub mod api;
pub mod auth;
pub mod generic;
pub mod middleware;
pub mod ui;
pub mod utils;

pub const STATUS_OK: &str = "Ok";

// TODO: look at the ServiceBuilder layers bits here: https://github.com/tokio-rs/axum/blob/dea36db400f27c025b646e5720b9a6784ea4db6e/examples/key-value-store/src/main.rs

type SharedState = Arc<RwLock<State>>;

#[async_trait]
pub trait SharedStateTrait {
    async fn connpool(&self) -> Pool<Sqlite>;
    async fn config(&self) -> ConfigFile;
    async fn oidc_config(&self) -> Option<CustomProviderMetadata>;
    async fn oidc_update(&self, response: CustomProviderMetadata);
    async fn pop_verifier(&self, csrftoken: String) -> Option<(PkceCodeVerifier, Nonce)>;
    async fn oauth2_client_id(&self) -> ClientId;
    async fn oauth2_secret(&self) -> Option<ClientSecret>;
    async fn push_verifier(&self, csrftoken: String, verifier: (PkceCodeVerifier, Nonce));
}

#[async_trait]
impl SharedStateTrait for SharedState {
    /// Get an sqlite connection pool
    async fn connpool(&self) -> Pool<Sqlite> {
        self.write().await.connpool.clone()
    }
    /// Get a copy of the config
    async fn config(&self) -> ConfigFile {
        self.read().await.config.clone()
    }
    /// Get a copy of the config
    async fn oidc_config(&self) -> Option<CustomProviderMetadata> {
        self.read().await.oidc_config.clone()
    }
    async fn oidc_update(&self, response: CustomProviderMetadata) {
        let mut writer = self.write().await;
        writer.oidc_config = Some(response.clone());
        writer.oidc_config_updated = Some(DateTime::from_utc(NaiveDateTime::default(), Utc));
    }

    async fn pop_verifier(&self, csrftoken: String) -> Option<(PkceCodeVerifier, Nonce)> {
        let mut writer = self.write().await;
        let result = writer.oidc_verifier.remove_entry(&csrftoken);
        result.map(|(_, (pkce, nonce))| (pkce, nonce))
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
    async fn push_verifier(&self, csrftoken: String, verifier: (PkceCodeVerifier, Nonce)) {
        let mut writer = self.write().await;
        writer.oidc_verifier.insert(csrftoken, verifier);
    }
}

#[derive(Debug)]
/// Internal State handler for the datastore object within the API
pub struct State {
    pub tx: Sender<datastore::Command>,
    pub connpool: Pool<Sqlite>,
    pub config: ConfigFile,
    pub oidc_config_updated: Option<DateTime<Utc>>,
    pub oidc_config: Option<auth::CustomProviderMetadata>,
    pub oidc_verifier: HashMap<String, (PkceCodeVerifier, Nonce)>,
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
    connpool: Pool<Sqlite>,
) -> Option<JoinHandle<Result<(), std::io::Error>>> {
    let static_dir: PathBuf = shellexpand::tilde(&config.api_static_dir)
        .to_string()
        .into();

    let session_layer = auth::build_auth_stores(config.clone(), connpool.clone()).await;

    let csp_matchers = vec![CspUrlMatcher::default_self(
        RegexSet::new([r"^(/|/ui)"]).unwrap(),
    )];

    // let config_clone: ConfigFile = ConfigFile::from(&config);
    let state: SharedState = Arc::new(RwLock::new(State {
        tx,
        connpool,
        config: (*config).clone(),
        oidc_config_updated: None,
        oidc_config: None,
        oidc_verifier: HashMap::new(),
        csp_matchers,
    }));

    // add u sum layerz https://docs.rs/tower-http/latest/tower_http/index.html
    let trace_layer =
        TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().level(Level::INFO));

    pub async fn handler_404() -> impl IntoResponse {
        axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-type", "text/html")
            .body(
                "<h1>Oh no!</h1><p>You've found a 404, try <a href='#' onclick='history.back();'>going back</a> or <a href='/'>home!</a></p>"
                    .to_string(),
            )
            .unwrap()
            .into_response()
    }

    let router = Router::new()
        .route("/", get(generic::index))
        .nest("/ui", ui::new())
        .nest("/api", api::new())
        .nest("/auth", auth::new())
        .layer(
            ServiceBuilder::new()
                .layer(trace_layer)
                .layer(Extension(state))
                .layer(from_fn(csp::cspheaders))
                .layer(session_layer)
                .into_inner(),
        )
        .route("/status", get(generic::status));

    let router = match check_static_dir_exists(&static_dir, &config) {
        true => router.merge(SpaRouter::new("/static", &static_dir)),
        false => router,
    };
    let router = router
        .layer(CompressionLayer::new())
        .fallback(handler_404.into_service());

    let tls_cert = &config.api_tls_cert.clone();
    let tls_config = RustlsConfig::from_pem_file(&tls_cert, &config.api_tls_key)
        .await
        .unwrap();

    let res = Some(tokio::spawn(
        axum_server::bind_rustls(config.api_listener_address(), tls_config)
            .serve(router.into_make_service()),
    ));
    println!(
        "Started Web server on https://{}",
        config.api_listener_address()
    );
    log::debug!(
        "Started Web server on https://{}",
        config.api_listener_address()
    );
    res
}

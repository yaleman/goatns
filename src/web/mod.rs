//! # Web things
//!
//! Uses axum/tower for protocol, askama for templating, confusion for the rest.
//!

#![allow(clippy::clone_on_copy)]
// ^ this is because the datetime in the goatchildState is a jerk
use crate::config::ConfigFile;
use crate::datastore;
use crate::error::GoatNsError;

// TODO: return the API docs use crate::web::api::docs::ApiDoc;
use crate::web::middleware::csp;
use async_trait::async_trait;
use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_fn_with_state;
use axum::response::Redirect;
use axum::routing::get;
use axum_csp::CspUrlMatcher;
#[cfg(not(test))]
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use concread::cowcell::asynch::CowCellReadTxn;

use oauth2::{ClientId, ClientSecret};
use openidconnect::Nonce;
use regex::RegexSet;
use sea_orm::DatabaseConnection;
use sea_orm::DatabaseTransaction;
use sea_orm::TransactionTrait;
use utoipa::OpenApi;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::{RwLock, mpsc::Receiver};
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tracing::{debug, error, info, trace, warn};
use utils::{Urls, handler_404};

use self::auth::CustomProviderMetadata;

pub(crate) use askama::Template;
pub(crate) use askama_web::WebTemplate;

pub mod constants;
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
    async fn connpool(&self) -> DatabaseConnection;
    async fn oidc_update<'life0>(&'life0 mut self, response: CustomProviderMetadata);
    async fn pop_verifier<'life0>(&'life0 mut self, csrftoken: String) -> Option<(String, Nonce)>;
    async fn oauth2_client_id(&self) -> ClientId;
    async fn oauth2_secret(&self) -> Option<ClientSecret>;
    async fn push_verifier(&mut self, csrftoken: String, verifier: (String, Nonce));
    async fn get_db_txn(&self) -> Result<DatabaseTransaction, Redirect>;
}

#[async_trait]
impl GoatStateTrait for GoatState {
    /// Get an sqlite connection pool
    async fn connpool(&self) -> DatabaseConnection {
        self.read().await.db.clone()
    }
    async fn oidc_update<'life0>(&'life0 mut self, response: CustomProviderMetadata) {
        debug!("Storing OIDC config!");
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
        trace!("Pushing CSRF token into shared state: token={csrftoken}");
        writer.oidc_verifier.insert(csrftoken, verifier);
    }

    async fn get_db_txn(&self) -> Result<DatabaseTransaction, Redirect> {
        self.read().await.db.begin().await.map_err(|err| {
            error!("Failed to begin DB transaction: {err:?}");
            Urls::Admin.redirect()
        })
    }
}

pub(crate) type GoatState = Arc<RwLock<GoatChildState>>;

#[derive(Clone, FromRef)]
/// Internal State handler for the datastore object within the API
pub struct GoatChildState {
    pub tx: Sender<datastore::Command>,
    pub db: DatabaseConnection,
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
                info!("Found static resources dir ({static_dir:#?}) for web API.");
                return true;
            }
            false => {
                error!("Couldn't find static resources dir ({static_dir:#?}) for web API!")
            }
        },
        Err(err) => match err.kind() {
            std::io::ErrorKind::PermissionDenied => {
                error!(
                    "Permission denied accssing static resources dir ({:?}) for web API: {}",
                    &config.api_static_dir,
                    err.to_string()
                )
            }
            std::io::ErrorKind::NotFound => {
                error!(
                    "Static resources dir ({:?}) not found for web API: {}",
                    &config.api_static_dir,
                    err.to_string()
                )
            }
            _ => error!(
                "Error accessing static resources dir ({:?}) for web API: {}",
                &config.api_static_dir,
                err.to_string()
            ),
        },
    }
    false
}

async fn build_router(
    tx: Sender<datastore::Command>,
    config: CowCellReadTxn<ConfigFile>,
    connpool: DatabaseConnection,
) -> Result<Router, GoatNsError> {
    let session_layer = auth::build_auth_stores(config.clone(), connpool.clone()).await?;

    let csp_matchers = vec![CspUrlMatcher::default_self(
        RegexSet::new([r"^(/|/ui)"]).map_err(|err| {
            GoatNsError::StartupError(format!("Failed to generate CSP matchers regex: {err}"))
        })?,
    )];

    // we set this to an hour ago so it forces update on startup
    let oidc_config_updated = Utc::now()
        - TimeDelta::try_hours(1).ok_or(GoatNsError::StartupError(
            "Failed to create TimeDelta for OIDC config update".to_string(),
        ))?;

    let state = Arc::new(RwLock::new(GoatChildState {
        tx,
        db: connpool.clone(),
        config: (*config).clone(),
        oidc_config_updated,
        oidc_config: None,
        oidc_verifier: HashMap::new(),
        csp_matchers,
    }));

    let service_layer = ServiceBuilder::new()
        .layer(session_layer)
        .layer(from_fn_with_state(state.clone(), csp::cspheaders));

    let router = Router::new()
        .route(&Urls::Home.to_string(), get(generic::index))
        .nest("/ui", ui::new())
        .nest("/api", api::new())
        .merge(
            utoipa_swagger_ui::SwaggerUi::new("/api/docs")
                .url("/api/openapi.json", api::docs::ApiDoc::openapi()),
        )
        .nest("/auth", auth::new())
        .nest("/dns-query", doh::new())
        .with_state(state)
        .layer(service_layer);

    // here we add the tracing layer

    #[cfg(not(test))]
    let router = router
        .layer(OtelInResponseLayer)
        .layer(OtelAxumLayer::default());

    let router = router.route("/status", get(generic::status));

    let router = match check_static_dir_exists(&config.static_path(), &config) {
        true => router.nest_service("/static", ServeDir::new(config.static_path())),
        false => {
            warn!(
                static_path = %config.static_path().display(),
                "Static path doesn't exist, disabling static file serving."
            );
            router
        }
    };
    Ok(router.layer(CompressionLayer::new()).fallback(handler_404))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ServerCommand {
    ReloadTls,
    ShutDown,
}

pub async fn build(
    tx: Sender<datastore::Command>,
    rx: Receiver<ServerCommand>,
    config: CowCellReadTxn<ConfigFile>,
    connpool: DatabaseConnection,
) -> Result<JoinHandle<Result<(), std::io::Error>>, GoatNsError> {
    let listener_address = config.api_listener_address()?;
    let listener = std::net::TcpListener::bind(listener_address)
        .map_err(|err| GoatNsError::StartupError(format!("Failed to bind API listener: {err}")))?;
    listener.set_nonblocking(true).map_err(|err| {
        GoatNsError::StartupError(format!("Failed to set API listener nonblocking: {err}"))
    })?;
    build_with_listener(tx, rx, config, connpool, listener).await
}

pub async fn build_with_listener(
    tx: Sender<datastore::Command>,
    rx: Receiver<ServerCommand>,
    config: CowCellReadTxn<ConfigFile>,
    connpool: DatabaseConnection,
    listener: std::net::TcpListener,
) -> Result<JoinHandle<Result<(), std::io::Error>>, GoatNsError> {
    let router = build_router(tx, config.clone(), connpool).await?;
    let listener_address: SocketAddr = listener.local_addr().map_err(|err| {
        GoatNsError::StartupError(format!("Failed to inspect API listener: {err}"))
    })?;
    let hostname = config.hostname.clone();
    let tls_cert = config.api_tls_cert.clone();
    let tls_key = config.api_tls_key.clone();
    let tls_config = config
        .get_tls_config()
        .await
        .map_err(GoatNsError::StartupError)?;
    let mut rx = rx;
    let res: JoinHandle<Result<(), std::io::Error>> = tokio::spawn(async move {
        let handle = axum_server::Handle::new();
        let server = axum_server::from_tcp_rustls(listener, tls_config.clone())?
            .handle(handle.clone())
            .serve(router.into_make_service());
        tokio::pin!(server);

        loop {
            tokio::select! {
                Some(action) = rx.recv() => match action {
                    ServerCommand::ReloadTls => {
                        match tls_config.reload_from_pem_file(&tls_cert, &tls_key).await {
                            Ok(()) => {
                                info!("Reloaded TLS configuration for API listener.");
                            }
                            Err(err) => {
                                error!("Failed to reload TLS configuration: {err}");
                            }
                        }
                    }
                    ServerCommand::ShutDown => {
                        info!("Shutting down web server.");
                        handle.shutdown();
                    }
                },
                res = &mut server => {
                    if let Err(err) = res {
                        error!("Web server error: {}", err);
                        return Err(err);
                    }
                    return Ok(());
                }
            }
        }
    });
    let startup_message = format!(
        "Started Web server on https://{} / https://{}:{}",
        listener_address,
        hostname,
        listener_address.port()
    );

    #[cfg(test)]
    println!("{startup_message}");
    info!("{}", startup_message);
    Ok(res)
}

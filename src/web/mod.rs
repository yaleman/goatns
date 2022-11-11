use crate::config::ConfigFile;
use crate::datastore;
use async_trait::async_trait;
use axum::routing::get;
use axum::{Extension, Router};
/// # Web things
///
/// Uses axum/tower for protocol, askama for templating, confusion for the rest.
///
/// Example using shared state: https://github.com/tokio-rs/axum/blob/axum-v0.5.17/examples/key-value-store/src/main.rs
use axum_extra::routing::SpaRouter;
use chrono::{DateTime, NaiveDateTime, Utc};
use oauth2::{ClientId, ClientSecret, PkceCodeVerifier, RedirectUrl};
use openidconnect::Nonce;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Level;
use url::Url;

use self::auth::CustomProviderMetadata;

#[macro_use]
pub mod macros;

pub mod api;
pub mod auth;
pub mod generic;
pub mod ui;

pub const STATUS_OK: &str = "Ok";

// TODO: look at the ServiceBuilder layers bits here: https://github.com/tokio-rs/axum/blob/dea36db400f27c025b646e5720b9a6784ea4db6e/examples/key-value-store/src/main.rs

type SharedState = Arc<RwLock<State>>;

#[async_trait]
trait SharedStateTrait {
    async fn connpool(&self) -> Pool<Sqlite>;
    async fn config(&self) -> ConfigFile;
    async fn oidc_config(&self) -> Option<CustomProviderMetadata>;
    async fn oidc_update(&self, response: CustomProviderMetadata);
    async fn pop_verifier(&self, csrftoken: String) -> Option<(PkceCodeVerifier, Nonce)>;
    async fn oauth2_client_id(&self) -> ClientId;
    async fn oauth2_secret(&self) -> Option<ClientSecret>;
    async fn oauth2_redirect_url(&self) -> RedirectUrl;
    // async fn oauth2_introspection_url(&self) -> IntrospectionUrl;
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

    async fn oauth2_redirect_url(&self) -> RedirectUrl {
        let config = self.config().await;
        let baseurl = match config.api_port {
            443 => format!("https://{}", config.hostname),
            _ => format!("https://{}:{}", config.hostname, config.api_port),
        };
        let url = Url::parse(&format!("{}/auth/login", baseurl))
            .expect("Failed to parse config into an OAuth Redirect URL");
        RedirectUrl::from_url(url)
    }

    // async fn oauth2_introspection_url(&self) -> IntrospectionUrl {
    //     let reader = self.read().await;
    //     let introspect_url = reader.oidc_config.as_ref().unwrap().token_endpoint.clone();
    //     let domain = introspect_url.domain().unwrap();
    //     let introspect_url =
    //         Url::parse(&format!("https://{domain}/oauth2/token/introspect")).unwrap();
    //     IntrospectionUrl::from_url(introspect_url)
    // }

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
}

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

    let (auth_layer, session_layer) = auth::build_auth_stores(&config, connpool.clone()).await;

    let state: SharedState = Arc::new(RwLock::new(State {
        tx,
        connpool,
        config,
        oidc_config_updated: None,
        oidc_config: None,
        oidc_verifier: HashMap::new(),
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
                .layer(trace_layer)
                .layer(CompressionLayer::new())
                .layer(Extension(state))
                .layer(Extension(auth_layer))
                .layer(session_layer)
                .into_inner(),
        )
}

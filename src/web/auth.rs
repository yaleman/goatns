use super::SharedState;
use crate::config::ConfigFile;
use askama::Template;
use axum::response::Html;
use axum::routing::get;
use axum::{Extension, Router};
use axum_macros::debug_handler;
use chrono::{DateTime, Utc};
use kanidm_proto::oauth2::OidcDiscoveryResponse;

pub async fn oidc_discover(config: &ConfigFile) -> Result<OidcDiscoveryResponse, String> {
    let response = reqwest::get(&config.oauth2_config_url)
        .await
        .map_err(|e| format!("{e:?}"))?;

    let oauth_config: OidcDiscoveryResponse =
        response.json().await.map_err(|e| format!("{e:?}"))?;
    log::debug!("{oauth_config:#?}");
    Ok(oauth_config)
}

#[derive(Template)]
#[template(path = "auth_login.html")]
struct AuthLogin {}

#[debug_handler]
pub async fn login(Extension(state): Extension<SharedState>) -> Result<Html<String>, ()> {
    // let config = &shared_state.read().unwrap().config.clone();
    // let discover_data = oidc_discover(config).await?;

    // let result = format!("{discover_data:#?}");
    // shared_state.write().unwrap().oidc_config = Some(Arc::new(discover_data));
    let now: DateTime<Utc> = Utc::now();
    let mut state_writer = state.write().await;
    state_writer.oidc_config_updated = Some(now);
    drop(state_writer);
    // Ok(format!("{:?}", state.read().await))

    let context = AuthLogin {};
    Ok(Html::from(context.render().unwrap()))
}

#[derive(Template)]
#[template(path = "auth_logout.html")]
struct AuthLogout {}

pub async fn logout(Extension(_shared_state): Extension<SharedState>) -> Result<Html<String>, ()> {
    let context = AuthLogout {};
    Ok(Html::from(context.render().unwrap()))
}
pub fn new() -> Router {
    Router::new()
        .route("/login", get(login))
        .route("/logout", get(logout))
}

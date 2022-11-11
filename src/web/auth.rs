use super::SharedState;
use crate::config::ConfigFile;
use crate::web::SharedStateTrait;
use askama::Template;
use axum::extract::Query;
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::{Extension, Router};
use axum_login::axum_sessions::extractors::WritableSession;
use axum_macros::debug_handler;
use chrono::{DateTime, Utc};
use oauth2::PkceCodeChallenge;
use serde::{Deserialize, Serialize};

use async_sqlx_session::SqliteSessionStore;
use axum_login::axum_sessions::SessionLayer;

use axum_login::{AuthLayer, AuthUser, SqliteStore};
use rand::Rng;
use sqlx::{Pool, Sqlite};

use openidconnect::{core::*, TokenResponse};

use openidconnect::reqwest::async_http_client;
use openidconnect::EmptyAdditionalProviderMetadata;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, /*ClientId, ClientSecret,*/ CsrfToken, IssuerUrl,
    Nonce, ProviderMetadata, /*OAuth2TokenResponse, RedirectUrl,*/ Scope,
};
#[derive(Debug, Default, Clone, sqlx::FromRow)]
pub struct User {
    id: i64,
    password_hash: String,
    #[allow(dead_code)]
    username: String,
    #[allow(dead_code)]
    display_name: String,
}

impl AuthUser for User {
    fn get_id(&self) -> String {
        format!("{}", self.id)
    }

    fn get_password_hash(&self) -> String {
        self.password_hash.clone()
    }
}

#[derive(Deserialize)]
pub struct LoginQuery {
    pub state: Option<String>,
    pub code: Option<String>,
}

#[allow(dead_code)]
type AuthContext = axum_login::extractors::AuthContext<User, SqliteStore<User>>;

#[derive(Template)]
#[template(path = "auth_login.html")]
struct AuthLogin {
    errors: Vec<String>,
    redirect_url: String,
}

#[derive(Default, Debug, Deserialize, Serialize)]

struct OIDCClaims {
    groups: Vec<String>,
}

pub type CustomProviderMetadata = ProviderMetadata<
    EmptyAdditionalProviderMetadata,
    CoreAuthDisplay,
    CoreClientAuthMethod,
    CoreClaimName,
    CoreClaimType,
    CoreGrantType,
    CoreJweContentEncryptionAlgorithm,
    CoreJweKeyManagementAlgorithm,
    CoreJwsSigningAlgorithm,
    CoreJsonWebKeyType,
    CoreJsonWebKeyUse,
    CoreJsonWebKey,
    CoreResponseMode,
    CoreResponseType,
    CoreSubjectIdentifierType,
>;

pub async fn oauth_get_discover(state: &SharedState) -> Result<CustomProviderMetadata, String> {
    let issuer_url = IssuerUrl::new(state.config().await.oauth2_config_url);
    CoreProviderMetadata::discover_async(issuer_url.unwrap(), async_http_client)
        .await
        .map_err(|e| format!("{e:?}"))
}

pub async fn oauth_start(state: &SharedState) -> Result<url::Url, String> {
    let last_updated: DateTime<Utc> = match state.read().await.oidc_config_updated {
        None => Utc::now(),
        Some(val) => val,
    };
    let now: DateTime<Utc> = Utc::now();

    let delta = now - last_updated;
    let provider_metadata: CustomProviderMetadata = match delta.num_minutes() > 5 {
        true => oauth_get_discover(state).await.unwrap(),
        false => {
            log::debug!("using cached OIDC discovery data");
            let meta = state
                .oidc_config()
                .await
                .unwrap_or(oauth_get_discover(state).await.unwrap());
            state.oidc_update(meta.clone()).await;
            meta
        }
    };
    log::debug!("provider metadata: {provider_metadata:?}");

    // Generate a PKCE challenge.
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        state.oauth2_client_id().await,
        state.oauth2_secret().await,
    )
    // This example will be running its own server at localhost:8080.
    // See below for the server implementation.
    .set_redirect_uri(state.oauth2_redirect_url().await);

    // Generate the authorization URL to which we'll redirect the user.
    let (authorize_url, csrf_state, nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        // This example is requesting access to the the user's profile including email.
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();
    state
        .push_verifier(csrf_state.secret().to_owned(), (pkce_verifier, nonce))
        .await;
    Ok(authorize_url)
}

#[debug_handler]
pub async fn login(
    query: Query<LoginQuery>,
    mut session: WritableSession,
    Extension(state): Extension<SharedState>,
) -> impl IntoResponse {
    let mut errors: Vec<String> = vec![];

    // check if we've got an existing, valid session
    if !session.is_expired() {
        if let Some(signed_in) = session.get("signed_in") {
            if signed_in {
                return Redirect::to("/ui").into_response();
            }
        }
    }

    if query.state.is_none() || query.code.is_none() {
        let auth_url = &oauth_start(&state).await.unwrap().to_string();
        return Redirect::to(auth_url).into_response();
    }

    // if we get the state and code back then we can go back to the server for a token
    // ref https://github.com/kanidm/kanidm/blob/master/kanidmd/testkit/tests/oauth2_test.rs#L276

    log::debug!("querying auth token");

    let auth_code = AuthorizationCode::new(query.code.clone().unwrap());

    let verifier = state.pop_verifier(query.state.clone().unwrap()).await;
    if verifier.is_none() {
        errors.push("Couldn't find a known session!".to_string());
    }

    let (pkce_verifier, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            log::error!("Couldn't find a sesssion, redirecting...");
            return Redirect::temporary("/auth/login").into_response();
        }
    };

    let provider_metadata = state.oidc_config().await.unwrap();

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        state.oauth2_client_id().await,
        state.oauth2_secret().await,
    )
    .set_redirect_uri(state.oauth2_redirect_url().await);

    // Now you can exchange it for an access token and ID token.
    let token_response = client
        .exchange_code(auth_code)
        // Set the PKCE code verifier.
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("{e:?}"))
        .unwrap();

    // Extract the ID token claims after verifying its authenticity and nonce.
    let id_token = token_response.id_token().unwrap();
    log::debug!("id_token: {id_token:?}");
    let allowed_algs = vec![
        CoreJwsSigningAlgorithm::EcdsaP256Sha256,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha384,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha512,
    ];
    let verifier = &client.id_token_verifier().set_allowed_algs(allowed_algs);
    match id_token.claims(verifier, &nonce) {
        Ok(claims) => {
            // check if they're in the database

            if let Some(email) = claims.email() {
                session.insert("email", email.to_string()).unwrap();
            } else if let Some(email) = claims.preferred_username() {
                session.insert("email", email.to_string()).unwrap();
            } else {
            }
            session
                .insert("oauth_user_id", claims.subject().to_string())
                .unwrap();
            session.insert("signed_in", true).unwrap();

            Redirect::to("/ui").into_response()
        }
        Err(error) => {
            log::error!("Failed to verify claim token: {error:?}");
            Redirect::to("/").into_response()
        }
    }
    // let context = AuthLogin {
    //     errors,
    //     redirect_url: authorize_url,
    // };
    // need to log the user in!
    // Response::builder().status(200).body(Html::from(context.render().unwrap())).into_response()
    // Html::from(context.render().unwrap()).into_response()
}

#[derive(Template)]
#[template(path = "auth_logout.html")]
struct AuthLogout {}

pub async fn logout(
    // Extension(_shared_state): Extension<SharedState>,
    mut session: WritableSession,
) -> impl IntoResponse {
    // let context = AuthLogout {};
    session.destroy();
    // Html::from(context.render().unwrap()).into_response()
    Redirect::to("/")
}

pub async fn build_auth_stores(
    config: &ConfigFile,
    connpool: Pool<Sqlite>,
) -> (
    AuthLayer<User, SqliteStore<User>>,
    SessionLayer<SqliteSessionStore>,
) {
    let mut secret: [u8; 64] = [0; 64];
    rand::thread_rng().fill(&mut secret);

    let user_store = SqliteStore::<User>::new(connpool.clone());
    let auth_layer = AuthLayer::new(user_store, &secret);

    let session_store = SqliteSessionStore::from_client(connpool).with_table_name("sessions");

    session_store
        .migrate()
        .await
        .expect("Could not migrate session store.");
    let session_layer = SessionLayer::new(session_store, &secret)
        .with_secure(true)
        .with_cookie_domain(config.hostname.clone());

    (auth_layer, session_layer)
}

pub fn new() -> Router {
    let mut secret: [u8; 64] = [0; 64];
    rand::thread_rng().fill(&mut secret);

    Router::new()
        .route("/login", get(login))
        .route("/logout", get(logout))
}

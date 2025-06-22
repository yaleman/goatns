use super::GoatState;
use crate::config::ConfigFile;
use crate::db::{DBEntity, User};
use crate::error::GoatNsError;
use crate::web::utils::Urls;
use crate::web::GoatStateTrait;
use crate::COOKIE_NAME;
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use chrono::{DateTime, Utc};
use concread::cowcell::asynch::CowCellReadTxn;
use oauth2::{PkceCodeChallenge, PkceCodeVerifier, RedirectUrl};

use openidconnect::EmptyAdditionalProviderMetadata;
use openidconnect::{
    core::*, ClaimsVerificationError, EmptyAdditionalClaims, IdTokenClaims, TokenResponse,
};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, CsrfToken, IssuerUrl, Nonce, ProviderMetadata, Scope,
};
use serde::Deserialize;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{session_store::ExpiredDeletion, sqlx::SqlitePool, SqliteStore};

// pub(crate) mod sessionstore;
pub mod traits;
use tower_sessions::{Expiry, Session, SessionManagerLayer};
use tracing::{debug, error, info, instrument, trace};
use traits::*;

#[derive(Deserialize)]
/// Parser for path bits
pub struct QueryForLogin {
    /// OAuth2 CSRF token
    pub state: Option<String>,
    /// OAuth2 code
    pub code: Option<String>,
    /// Where we'll redirect users to after successful login
    pub redirect: Option<String>,
}

/// Used in the parsing of the OIDC Provider metadata
pub type CustomProviderMetadata = ProviderMetadata<
    EmptyAdditionalProviderMetadata,
    CoreAuthDisplay,
    CoreClientAuthMethod,
    CoreClaimName,
    CoreClaimType,
    CoreGrantType,
    CoreJweContentEncryptionAlgorithm,
    CoreJweKeyManagementAlgorithm,
    CoreJsonWebKey,
    CoreResponseMode,
    CoreResponseType,
    CoreSubjectIdentifierType,
>;
type CustomClaimType = IdTokenClaims<EmptyAdditionalClaims, CoreGenderClaim>;

#[derive(Template, WebTemplate)]
#[template(path = "auth_login.html.j2")]
#[allow(dead_code)]
struct AuthLoginTemplate {
    errors: Vec<String>,
    redirect_url: String,
    pub user_is_admin: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "auth_new_user.html")]
struct AuthNewUserTemplate {
    state: String,
    code: String,
    email: String,
    displayname: String,
    redirect_url: String,
    errors: Vec<String>,
    pub user_is_admin: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "auth_logout.html")]
#[allow(dead_code)]
struct AuthLogoutTemplate {
    pub user_is_admin: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "auth_provisioning_disabled.html")]
/// This renders a page telling the user that auto-provisioning is disabled and to tell the admin which username to add
struct AuthProvisioningDisabledTemplate {
    username: String,
    authref: String,

    admin_contact_name: String,
    admin_contact_url: String,
    pub user_is_admin: bool,
}

pub enum ParserError {
    Redirect { content: Redirect },
    ErrorMessage { content: String },
    ClaimsVerificationError { content: ClaimsVerificationError },
}

/// Pull the OIDC Discovery details
#[instrument(level = "debug", skip(state))]
pub async fn oauth_get_discover(
    state: &mut GoatState,
) -> Result<CustomProviderMetadata, GoatNsError> {
    let issuer_url = IssuerUrl::new(state.read().await.config.oauth2_config_url.clone())
        .map_err(|err| GoatNsError::Oidc(err.to_string()))?;

    let http_client = get_http_client()?;

    match CoreProviderMetadata::discover_async(issuer_url, &http_client).await {
        Err(e) => Err(GoatNsError::Oidc(e.to_string())),
        Ok(val) => {
            state.oidc_update(val.clone()).await;
            Ok(val)
        }
    }
}

pub async fn oauth_start(state: &mut GoatState) -> Result<url::Url, GoatNsError> {
    let last_updated: DateTime<Utc> = state.read().await.oidc_config_updated;
    let now: DateTime<Utc> = Utc::now();

    let delta = now - last_updated;
    let discovery = oauth_get_discover(state).await?;

    let provider_metadata: CustomProviderMetadata = match delta.num_minutes() > 5 {
        true => discovery,
        false => {
            debug!("Using cached OIDC discovery data");
            let config = state.read().await.oidc_config.clone();
            let meta = config.unwrap_or(discovery);
            state.oidc_update(meta.clone()).await;
            meta
        }
    };
    trace!("provider metadata: {provider_metadata:?}");

    // Generate a PKCE challenge.
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        state.oauth2_client_id().await,
        state.oauth2_secret().await,
    )
    // This example will be running its own server at localhost:8080.
    // See below for the server implementation.
    .set_redirect_uri(RedirectUrl::from_url(
        state.read().await.config.oauth2_redirect_url.clone(),
    ));

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
        .push_verifier(
            csrf_state.secret().to_owned(),
            (pkce_verifier.secret().to_owned(), nonce),
        )
        .await;
    Ok(authorize_url)
}

pub fn get_http_client() -> Result<openidconnect::reqwest::Client, reqwest::Error> {
    openidconnect::reqwest::Client::builder()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
}

pub async fn parse_state_code(
    shared_state: &GoatState,
    query_code: String,
    pkce_verifier: PkceCodeVerifier,
    nonce: Nonce,
) -> Result<CustomClaimType, ParserError> {
    let auth_code = AuthorizationCode::new(query_code);
    let reader = shared_state.read().await;
    let provider_metadata = match &reader.oidc_config {
        Some(value) => value,
        None => {
            return Err(ParserError::ErrorMessage {
                content: "Failed to pull provider metadata!".to_string(),
            });
        }
    };

    let client = CoreClient::from_provider_metadata(
        provider_metadata.to_owned(),
        shared_state.oauth2_client_id().await,
        shared_state.oauth2_secret().await,
    )
    .set_redirect_uri(RedirectUrl::from_url(
        shared_state.read().await.config.oauth2_redirect_url.clone(),
    ));
    let verifier_copy = PkceCodeVerifier::new(pkce_verifier.secret().clone());
    assert_eq!(verifier_copy.secret(), pkce_verifier.secret());

    let http_client = get_http_client().map_err(|err| ParserError::ErrorMessage {
        content: format!(
            "Failed to build reqwest client to query OIDC token response: {:?}",
            err
        ),
    })?;

    // Now you can exchange it for an access token and ID token.
    let token_response = client
        .exchange_code(auth_code)
        .map_err(|err| ParserError::ErrorMessage {
            content: format!("{err:?}"),
        })?
        // Set the PKCE code verifier.
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| ParserError::ErrorMessage {
            content: format!("{e:?}"),
        })?;

    // Extract the ID token claims after verifying its authenticity and nonce.
    let id_token = match token_response.id_token() {
        Some(token) => token,
        None => {
            return Err(ParserError::ErrorMessage {
                content: "couldn't parse token".to_string(),
            })
        }
    };
    trace!("id_token: {id_token:?}");
    let allowed_algs = vec![
        CoreJwsSigningAlgorithm::EcdsaP256Sha256,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha384,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha512,
    ];
    let verifier = &client.id_token_verifier().set_allowed_algs(allowed_algs);
    // if verifier.is_none() {
    // return Err(ParserError::ErrorMessage{content: "Couldn't find a known session!"});
    // }
    id_token
        .claims(verifier, &nonce)
        .map_err(|e| ParserError::ClaimsVerificationError { content: e })
        .cloned()
}

pub async fn login(
    Query(query): Query<QueryForLogin>,
    session: Session,
    State(mut state): State<GoatState>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // check if we've got an existing, valid session
    if let Some(signed_in) = session.get("signed_in").await.unwrap_or(Some(false)) {
        if signed_in {
            return Ok(Urls::Dashboard.redirect().into_response());
        }
    }

    let (query_state, query_code) = match (query.state, query.code) {
        (Some(state), Some(code)) => (state, code),
        _ => {
            let auth_url = &oauth_start(&mut state)
                .await
                .map_err(|err| {
                    error!("Failed to do OIDC Discovery: {err:?}");
                    (StatusCode::INTERNAL_SERVER_ERROR, "OIDC Discovery Error").into_response()
                })?
                .to_string();
            return Ok(Redirect::to(auth_url).into_response());
        }
    };

    // if we get the state and code back then we can go back to the server for a token
    // ref <https://github.com/kanidm/kanidm/blob/master/kanidmd/testkit/tests/oauth2_test.rs#L276>

    let verifier = state.pop_verifier(query_state.clone()).await;

    let (pkce_verifier_secret, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            error!("Couldn't find a session, redirecting...");
            return Err(Urls::Login.redirect().into_response());
        }
    };
    let verifier_copy = PkceCodeVerifier::new(pkce_verifier_secret.clone());

    let claims = parse_state_code(
        &state,
        query_code.clone(),
        PkceCodeVerifier::new(pkce_verifier_secret),
        nonce.clone(),
    )
    .await;
    match claims {
        Ok(claims) => {
            // check if they're in the database

            let email = claims.get_email().map_err(|err| err.into_response())?;

            let mut dbuser = match User::get_by_subject(&state.connpool().await, claims.subject())
                .await
            {
                Ok(Some(user)) => user,
                Ok(None) => {
                    if !state.read().await.config.user_auto_provisioning {
                        // TODO: show a "sorry" page when auto-provisioning's not enabled
                        // warn!("User attempted login when auto-provisioning is not enabled, yeeting them to the home page.");
                        let (admin_contact_name, admin_contact_url) =
                            state.read().await.config.admin_contact.to_html_parts();

                        return Ok(AuthProvisioningDisabledTemplate {
                            username: claims.get_username(),
                            authref: claims.subject().to_string(),
                            admin_contact_name,
                            admin_contact_url,
                            user_is_admin: false, // TODO: ... probably not an admin but we can check
                        }
                        .into_response());
                    }
                    // push it back into the stack for signup

                    state
                        .push_verifier(
                            query_state.clone(),
                            (verifier_copy.secret().to_owned(), nonce),
                        )
                        .await;

                    return Ok(AuthNewUserTemplate {
                        state: query_state,
                        code: query_code,
                        email,
                        displayname: claims.get_displayname(),
                        redirect_url: "".to_string(),
                        errors: vec![],
                        user_is_admin: false, // TODO: ... probably not an admin but we can check
                    }
                    .into_response());
                }
                Err(error) => {
                    error!("Database error finding user {:?}: {error:?}", email.clone());
                    let redirect: Option<String> = session.remove("redirect").await.unwrap_or(None);
                    return match redirect {
                        Some(destination) => Err(Redirect::to(&destination).into_response()),
                        None => Err(Urls::Home.redirect().into_response()),
                    };
                }
            };
            debug!("Found user in database: {dbuser:?}");

            if dbuser.disabled {
                session.flush().await.map_err(|err| {
                    error!("Failed to flush session: {err:?}");
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to flush session store!",
                    )
                        .into_response()
                })?;
                info!("Disabled user attempted to log in: {dbuser:?}");
                return Err(Urls::Home.redirect().into_response());
            }

            if let Some(claims_email) = claims.email() {
                let claims_email = claims_email.to_string();
                if claims_email != dbuser.email {
                    debug!(
                        "Email doesn't match on login: {} != {}",
                        claims_email, dbuser.email
                    );

                    dbuser.email = claims_email;
                    let mut db_txn = match state.connpool().await.begin().await {
                        Ok(val) => val,
                        Err(err) => {
                            error!("Failed to start transaction to store user: {err:?}");
                            return Err(Urls::Home.redirect().into_response());
                            // TODO: this probably... should be handled as a better error?
                        }
                    };
                    if let Err(err) = dbuser.update_with_txn(&mut db_txn).await {
                        error!("Failed to update user email: {err:?}");
                        return Err(Urls::Home.redirect().into_response());
                        // TODO: this probably... should be handled as a better error?
                    }
                    if let Err(err) = db_txn.commit().await {
                        error!("Failed to commit user email update: {err:?}");
                        return Err(Urls::Home.redirect().into_response());
                    };
                }
            }

            if session
                .insert("authref", claims.subject().to_string())
                .await
                .is_err()
            {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to store session details",
                )
                    .into_response());
            };
            if session.insert("user", dbuser).await.is_err() {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to store session details",
                )
                    .into_response());
            };
            if session.insert("signed_in", true).await.is_err() {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to store session details",
                )
                    .into_response());
            };

            Ok(Urls::Dashboard.redirect().into_response())
        }
        Err(error) => match error {
            ParserError::Redirect { content } => Ok(content.into_response()),
            ParserError::ErrorMessage { content } => {
                debug!("Failed to parse state: {content}");
                Err(Urls::Home.redirect().into_response())
            }
            ParserError::ClaimsVerificationError { content } => {
                error!("Failed to verify claim token: {content:?}");
                Err(Urls::Home.redirect().into_response())
            }
        },
    }
}

pub async fn logout(session: Session) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    session.flush().await.map_err(|err| {
        error!("Failed to flush session: {err:?}");
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to flush session store!",
        )
    })?;
    Ok(Urls::Home.redirect())
}

pub async fn build_auth_stores(
    config: CowCellReadTxn<ConfigFile>,
    connpool: SqlitePool,
) -> Result<SessionManagerLayer<SqliteStore>, GoatNsError> {
    let session_store = SqliteStore::new(connpool)
        .with_table_name("sessions")
        .map_err(|err| {
            GoatNsError::StartupError(format!("Failed to initialize session store: {}", err))
        })?;

    session_store.migrate().await?;

    let _deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
    );

    Ok(SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::minutes(5)))
        .with_name(COOKIE_NAME)
        .with_secure(true)
        // If the cookies start being weird it's because they were appending a "." on the start...
        .with_domain(config.hostname.clone()))
}

#[derive(Deserialize, Debug)]
/// This handles the POST from "would you like to create your user"
pub struct SignupForm {
    pub state: String,
    pub code: String,
}

/// /auth/signup
pub async fn signup(
    State(mut state): State<GoatState>,
    Form(form): Form<SignupForm>,
) -> Result<Response, Redirect> {
    debug!("Dumping form: {form:?}");

    let query_state = form.state;

    let verifier = state.pop_verifier(query_state).await;

    let (pkce_verifier, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            error!("Couldn't find a signup session, redirecting user...");
            return Ok(Urls::Login.redirect().into_response());
        }
    };
    let claims = parse_state_code(
        &state,
        form.code,
        PkceCodeVerifier::new(pkce_verifier),
        nonce.clone(),
    )
    .await;
    match claims {
        Err(error) => match error {
            ParserError::Redirect { content } => Ok(content.into_response()),
            ParserError::ErrorMessage { content } => {
                error!("Failed to parse claim: {}", content);
                Ok(Urls::Home.redirect().into_response())
            }
            ParserError::ClaimsVerificationError { content } => {
                error!("Failed to verify claim token: {content:?}");
                Ok(Urls::Home.redirect().into_response())
            }
        },
        Ok(claims) => {
            debug!("Verified claims in signup form: {claims:?}");
            let user = User {
                id: None,
                displayname: claims.get_displayname(),
                username: claims.get_username(),
                email: claims.get_email()?,
                disabled: false,
                authref: Some(claims.subject().to_string()),
                admin: false,
            };
            match user.save(&state.connpool().await).await {
                Ok(_) => Ok(Urls::Dashboard.redirect().into_response()),
                Err(error) => {
                    debug!("Failed to save new user signup... oh no! {error:?}");
                    // TODO: throw an error page on this one
                    Ok(Urls::Home.redirect().into_response())
                }
            }
        }
    }
}

pub fn new() -> Router<GoatState> {
    Router::new()
        .route("/login", get(login))
        .route("/logout", get(logout))
        .route("/signup", post(signup))
}

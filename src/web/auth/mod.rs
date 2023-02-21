use std::time::Duration;

use super::utils::{redirect_to_dashboard, redirect_to_home};
use super::GoatState;
use crate::config::ConfigFile;
use crate::db::{DBEntity, User};
use crate::web::GoatStateTrait;
use crate::COOKIE_NAME;

use askama::Template;
use async_sqlx_session::SqliteSessionStore;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
// use axum_macros::debug_handler;
use axum_sessions::extractors::WritableSession;
use axum_sessions::SessionLayer;
use chrono::{DateTime, Utc};
use concread::cowcell::asynch::CowCellReadTxn;
use oauth2::{PkceCodeChallenge, PkceCodeVerifier, RedirectUrl};
use openidconnect::reqwest::async_http_client;
use openidconnect::EmptyAdditionalProviderMetadata;
use openidconnect::{
    core::*, ClaimsVerificationError, EmptyAdditionalClaims, IdTokenClaims, TokenResponse,
};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, CsrfToken, IssuerUrl, Nonce, ProviderMetadata, Scope,
};
use serde::Deserialize;
use sqlx::{Pool, Sqlite};

pub mod sessions;
pub mod traits;
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
    CoreJwsSigningAlgorithm,
    CoreJsonWebKeyType,
    CoreJsonWebKeyUse,
    CoreJsonWebKey,
    CoreResponseMode,
    CoreResponseType,
    CoreSubjectIdentifierType,
>;
type CustomClaimType = IdTokenClaims<EmptyAdditionalClaims, CoreGenderClaim>;

#[derive(Template)]
#[template(path = "auth_login.html")]
struct AuthLoginTemplate {
    errors: Vec<String>,
    redirect_url: String,
}

#[derive(Template)]
#[template(path = "auth_new_user.html")]
struct AuthNewUserTemplate {
    state: String,
    code: String,
    email: String,
    displayname: String,
    redirect_url: String,
    errors: Vec<String>,
}

#[derive(Template)]
#[template(path = "auth_logout.html")]
struct AuthLogoutTemplate {}

#[derive(Template)]
#[template(path = "auth_provisioning_disabled.html")]
/// This renders a page telling the user that auto-provisioning is disabled and to tell the admin which username to add
struct AuthProvisioningDisabledTemplate {
    username: String,
    authref: String,
    admin_contact: String,
}

pub enum ParserError {
    Redirect { content: Redirect },
    ErrorMessage { content: &'static str },
    ClaimsVerificationError { content: ClaimsVerificationError },
}

/// Pull the OIDC Discovery details
pub async fn oauth_get_discover(state: &mut GoatState) -> Result<CustomProviderMetadata, String> {
    log::debug!("Getting discovery data");
    let issuer_url = IssuerUrl::new(state.read().await.config.oauth2_config_url.clone());
    match CoreProviderMetadata::discover_async(issuer_url.unwrap(), async_http_client).await {
        Err(e) => Err(format!("{e:?}")),
        Ok(val) => {
            state.oidc_update(val.clone()).await;
            Ok(val)
        }
    }
}

pub async fn oauth_start(state: &mut GoatState) -> Result<url::Url, String> {
    let last_updated: DateTime<Utc> = state.read().await.oidc_config_updated;
    let now: DateTime<Utc> = Utc::now();

    let delta = now - last_updated;
    let provider_metadata: CustomProviderMetadata = match delta.num_minutes() > 5 {
        true => oauth_get_discover(state).await.unwrap(),
        false => {
            log::debug!("using cached OIDC discovery data");
            let config = state.read().await.oidc_config.clone();
            let meta = config.unwrap_or(oauth_get_discover(state).await.unwrap());
            state.oidc_update(meta.clone()).await;
            meta
        }
    };
    log::trace!("provider metadata: {provider_metadata:?}");

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
                content: "Failed to pull provider metadata!",
            })
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
    let id_token = match token_response.id_token() {
        Some(token) => token,
        None => {
            return Err(ParserError::ErrorMessage {
                content: "couldn't parse token",
            })
        }
    };
    log::trace!("id_token: {id_token:?}");
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

// #[debug_handler]
pub async fn login(
    Query(query): Query<QueryForLogin>,
    mut session: WritableSession,
    axum::extract::State(mut state): axum::extract::State<GoatState>,
) -> impl IntoResponse {
    // check if we've got an existing, valid session
    if !session.is_expired() {
        if let Some(signed_in) = session.get("signed_in") {
            if signed_in {
                return Redirect::to("/ui").into_response();
            }
        }
    }

    if query.state.is_none() || query.code.is_none() {
        let auth_url = &oauth_start(&mut state).await.unwrap().to_string();
        return Redirect::to(auth_url).into_response();
    }

    // if we get the state and code back then we can go back to the server for a token
    // ref https://github.com/kanidm/kanidm/blob/master/kanidmd/testkit/tests/oauth2_test.rs#L276

    let verifier = state.pop_verifier(query.state.clone().unwrap()).await;

    let (pkce_verifier_secret, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            log::error!("Couldn't find a sesssion, redirecting...");
            return Redirect::to("/auth/login").into_response();
        }
    };
    let verifier_copy = PkceCodeVerifier::new(pkce_verifier_secret.clone());

    let claims = parse_state_code(
        &state,
        query.code.clone().unwrap(),
        PkceCodeVerifier::new(pkce_verifier_secret),
        nonce.clone(),
    )
    .await;
    match claims {
        Ok(claims) => {
            // check if they're in the database

            let email = claims.get_email().unwrap();

            let dbuser = match User::get_by_subject(&state.connpool().await, claims.subject()).await
            {
                Ok(user) => user,
                Err(error) => {
                    match error {
                        sqlx::Error::RowNotFound => {
                            if !state.read().await.config.user_auto_provisioning {
                                // TODO: show a "sorry" page when auto-provisioning's not enabled
                                // log::warn!("User attempted login when auto-provisioning is not enabled, yeeting them to the home page.");
                                let admin_contact =
                                    state.read().await.config.admin_contact.to_string();
                                // let admin_contact = match admin_contact {
                                //     Some(value) => value.to_string(),
                                //     None => "the administrator".to_string(),
                                // };
                                let context = AuthProvisioningDisabledTemplate {
                                    username: claims.get_username(),
                                    authref: claims.subject().to_string(),
                                    admin_contact,
                                };
                                return Response::builder()
                                    .status(200)
                                    .body(context.render().unwrap())
                                    .unwrap()
                                    .into_response();
                            }

                            let new_user_page = AuthNewUserTemplate {
                                state: query.state.clone().unwrap(),
                                code: query.code.clone().unwrap(),
                                email,
                                displayname: claims.get_displayname(),
                                redirect_url: "".to_string(),
                                errors: vec![],
                            };
                            let pagebody = new_user_page.render().unwrap();
                            // push it back into the stack for signup
                            state
                                .push_verifier(
                                    query.state.clone().unwrap(),
                                    (verifier_copy.secret().to_owned(), nonce),
                                )
                                .await;

                            return Response::builder()
                                .status(200)
                                .body(pagebody)
                                .unwrap()
                                .into_response();
                        }
                        _ => {
                            log::error!(
                                "Database error finding user {:?}: {error:?}",
                                email.clone()
                            );
                            let redirect: Option<String> = session.get("redirect");
                            return match redirect {
                                Some(destination) => {
                                    session.remove("redirect");
                                    Redirect::to(&destination).into_response()
                                }
                                None => redirect_to_home().into_response(),
                            };
                        }
                    }
                }
            };
            log::debug!("Found user in database: {dbuser:?}");

            if dbuser.disabled {
                session.destroy();
                log::info!("Disabled user attempted to log in: {dbuser:?}");
                return redirect_to_home().into_response();
            }

            session
                .insert("authref", claims.subject().to_string())
                .unwrap();
            session.insert("user", dbuser).unwrap();
            session.insert("signed_in", true).unwrap();

            redirect_to_dashboard().into_response()
        }
        Err(error) => match error {
            ParserError::Redirect { content } => content.into_response(),
            ParserError::ErrorMessage { content } => {
                log::debug!("Failed to parse state: {content}");
                todo!();
            }
            ParserError::ClaimsVerificationError { content } => {
                log::error!("Failed to verify claim token: {content:?}");
                redirect_to_home().into_response()
            }
        },
    }
}

pub async fn logout(mut session: WritableSession) -> impl IntoResponse {
    session.destroy();
    Redirect::to("/")
}

pub async fn build_auth_stores(
    config: CowCellReadTxn<ConfigFile>,
    connpool: Pool<Sqlite>,
) -> SessionLayer<SqliteSessionStore> {
    let session_store = SqliteSessionStore::from_client(connpool).with_table_name("sessions");

    session_store
        .migrate()
        .await
        .expect("Could not migrate session store database on startup!");

    tokio::spawn(sessions::session_store_cleanup(
        Duration::from_secs(config.sql_db_cleanup_seconds.to_owned()),
        session_store.clone(),
    ));

    SessionLayer::new(session_store, config.api_cookie_secret())
        .with_secure(true)
        // TODO: cookie domain isn't working because it sets .(hostname) for some reason.
        // .with_cookie_domain(config.hostname.clone())
        .with_cookie_name(COOKIE_NAME)
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
) -> Result<Response, ()> {
    log::debug!("Dumping form: {form:?}");

    let query_state = form.state;

    let verifier = state.pop_verifier(query_state).await;

    let (pkce_verifier, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            log::error!("Couldn't find a signup session, redirecting user...");
            return Ok(Redirect::to("/auth/login").into_response());
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
                log::debug!("{content}");
                todo!();
            }
            ParserError::ClaimsVerificationError { content } => {
                log::error!("Failed to verify claim token: {content:?}");
                Ok(redirect_to_home().into_response())
            }
        },
        Ok(claims) => {
            log::debug!("Verified claims in signup form: {claims:?}");
            let email = claims.get_email().unwrap();
            let user = User {
                id: None,
                displayname: claims.get_displayname(),
                username: claims.get_username(),
                email,
                disabled: false,
                authref: Some(claims.subject().to_string()),
                admin: false,
            };
            match user.save(&state.connpool().await).await {
                Ok(_) => Ok(redirect_to_dashboard().into_response()),
                Err(error) => {
                    log::debug!("Failed to save new user signup... oh no! {error:?}");
                    // TODO: throw an error page on this one
                    Ok(redirect_to_home().into_response())
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

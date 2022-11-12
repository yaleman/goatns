use super::SharedState;
use crate::config::ConfigFile;
use crate::db::{User, DBEntity};
use crate::web::SharedStateTrait;
use crate::COOKIE_NAME;
use super::utils::{redirect_to_home, redirect_to_dashboard};
use askama::Template;
use axum::extract::Query;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use axum_login::axum_sessions::extractors::WritableSession;
use axum_macros::debug_handler;
use chrono::{DateTime, Utc};
use oauth2::{PkceCodeChallenge, PkceCodeVerifier};
use serde::Deserialize;

use async_sqlx_session::SqliteSessionStore;
use axum_login::axum_sessions::SessionLayer;

use axum_login::{AuthLayer, SqliteStore};
use rand::Rng;
use sqlx::{Pool, Sqlite};

use openidconnect::{core::*, TokenResponse, ClaimsVerificationError, EndUserUsername, IdTokenClaims, EmptyAdditionalClaims};

use openidconnect::reqwest::async_http_client;
use openidconnect::EmptyAdditionalProviderMetadata;
use openidconnect::{AuthenticationFlow, AuthorizationCode, CsrfToken, IssuerUrl,Nonce, ProviderMetadata, Scope,};

#[derive(Deserialize)]
pub struct LoginQuery {
    /// OAuth2 CSRF token
    pub state: Option<String>,
    /// OAuth2 code
    pub code: Option<String>,
    /// Where we'll redirect users to after successful login
    pub redirect: Option<String>,
}

#[allow(dead_code)]
type AuthContext = axum_login::extractors::AuthContext<User, SqliteStore<User>>;

#[derive(Template)]
#[template(path = "auth_login.html")]
struct AuthLogin {
    errors: Vec<String>,
    redirect_url: String,
}

#[derive(Template)]
#[template(path = "auth_new_user.html")]
struct AuthNewUser {
    state: String,
    code: String,
    email: String,
    displayname: String,
    redirect_url: String,
    errors: Vec<String>,
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


pub enum ParserError {
    Redirect { content: Redirect },
    ErrorMessage { content: &'static str },
    ClaimsVerificationError { content: ClaimsVerificationError },
}

type CustomClaimType = IdTokenClaims<EmptyAdditionalClaims, CoreGenderClaim>;

pub async fn parse_state_code(
    shared_state: &SharedState,
    query_code: String,
    pkce_verifier: PkceCodeVerifier,
    nonce: Nonce,
) -> Result<
        CustomClaimType,
        ParserError,
> {
    let auth_code = AuthorizationCode::new(query_code);

    let provider_metadata = shared_state.oidc_config().await.unwrap();

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        shared_state.oauth2_client_id().await,
        shared_state.oauth2_secret().await,
    )
    .set_redirect_uri(shared_state.oauth2_redirect_url().await);
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
    let id_token = token_response.id_token().unwrap();
    log::debug!("id_token: {id_token:?}");
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
    id_token.claims(verifier, &nonce).map_err(|e| ParserError::ClaimsVerificationError { content: e }).cloned()
}


pub trait CustomClaimTypeThings {
    fn get_displayname(&self) -> String;
    fn get_email(&self) -> Result<String, Redirect>;
    fn get_username(&self) -> String;
}

impl CustomClaimTypeThings for CustomClaimType {

fn get_email(&self) -> Result<String, Redirect> {
    let email: String;
    if let Some(user_email) = self.email() {
        email = user_email.to_string();
    } else if let Some(user_email) = self.preferred_username() {
        email = user_email.to_string();
    } else {
        log::error!("Couldn't extract email address from claim: {self:?}");
        return Err(redirect_to_home());
    }
    Ok(email)
}
    fn get_displayname(&self) -> String {
        let mut displayname: String = "Anonymous Kid".to_string();
        if let Some(name) = self.name() {
            if let Some(username) = name.iter().next() {
                displayname = username.1.to_string();
            }
        }
        displayname
    }
    fn get_username(&self) -> String {

        let default = EndUserUsername::new("".to_string());
        self.preferred_username().unwrap_or(&default).to_string()
    }
}


#[debug_handler]
pub async fn login(
    query: Query<LoginQuery>,
    mut session: WritableSession,
    Extension(state): Extension<SharedState>,
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
        let auth_url = &oauth_start(&state).await.unwrap().to_string();
        return Redirect::to(auth_url).into_response();
    }

    // if we get the state and code back then we can go back to the server for a token
    // ref https://github.com/kanidm/kanidm/blob/master/kanidmd/testkit/tests/oauth2_test.rs#L276

    let verifier = state.pop_verifier(query.state.clone().unwrap()).await;

    let (pkce_verifier, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            log::error!("Couldn't find a sesssion, redirecting...");
            return Redirect::to("/auth/login").into_response();
        }
    };
    let verifier_copy = PkceCodeVerifier::new(pkce_verifier.secret().clone());

    let claims = parse_state_code(
        &state,
        query.code.clone().unwrap(),
        pkce_verifier,
        nonce.clone(),
    )
    .await;
    match claims {
        Ok(claims) => {
            // check if they're in the database

            let email = claims.get_email().map_err(|e| return e.into_response()).unwrap();

            let dbuser = match User::get_by_subject(&state.connpool().await, claims.subject()).await {
                Ok(user) => user,
                Err(error) => {
                    match error {
                        sqlx::Error::RowNotFound => {
                            // TODO: show the user a signup page?
                            let new_user_page = AuthNewUser {
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
                                .push_verifier(query.state.clone().unwrap(), (verifier_copy, nonce))
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
                                },
                                None => redirect_to_home().into_response()
                            }
                        }
                    }
                }
            };
            log::debug!("Found user in database: {dbuser:?}");

            if dbuser.disabled {
                session.destroy();
                log::info!("Disabled user attempted to log in: {dbuser:?}");
                // TODO: show a disabled user prompt
                return redirect_to_home().into_response();
            }

            session
                .insert("authref", claims.subject().to_string())
                .unwrap();
            session.insert("signed_in", true).unwrap();

            redirect_to_dashboard().into_response()
        }
        Err(error) => {
            match error
            {
                ParserError::Redirect { content } => return content.into_response(),
                ParserError::ErrorMessage { content } => {
                    log::debug!("{content}");
                    todo!();
                },
                ParserError::ClaimsVerificationError { content } => {
                    log::error!("Failed to verify claim token: {content:?}");
                    redirect_to_home().into_response()
                }
            }
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
    _config: &ConfigFile,
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
        // TODO: this isn't working because it sets .(hostname) for some reason.
        // .with_cookie_domain(config.hostname.clone())
        .with_cookie_name(COOKIE_NAME);

    (auth_layer, session_layer)
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]

pub struct SignupForm {
    pub state: String,
    pub code: String,
}

pub async fn signup(
    Extension(state): Extension<SharedState>,
    Form(form): Form<SignupForm>,
) -> impl IntoResponse {
    log::debug!("Dumping form: {form:?}");

    let query_state = form.state;

    let verifier = state.pop_verifier(query_state).await;

    let (pkce_verifier, nonce) = match verifier {
        Some((p, n)) => (p, n),
        None => {
            log::error!("Couldn't find a signup session, redirecting user...");
            return Redirect::to("/auth/login").into_response();
        }
    };
    // let verifier_copy = PkceCodeVerifier::new(pkce_verifier.secret().clone());

    let claims = parse_state_code(
        &state,
        form.code,
        pkce_verifier,
        nonce.clone(),
    )
    .await;
    match claims {
        Err(error) => match error {
            ParserError::Redirect { content } => return content.into_response(),
            ParserError::ErrorMessage { content } => {
                log::debug!("{content}");
                todo!();
            },
            ParserError::ClaimsVerificationError { content } => {
                log::error!("Failed to verify claim token: {content:?}");
                return redirect_to_home().into_response()
            }
        },
        Ok(claims) => {
            log::debug!("Verified claims in signup form: {claims:?}");
            let email = claims.get_email().map_err(|e| return e.into_response()).unwrap();
            let user = User {
                id: None,
                displayname: claims.get_displayname(),
                username: claims.get_username(),
                email,
                disabled: false,
                authref: Some(claims.subject().to_string()),
            };
            match user.save(&state.connpool().await).await {
                Ok(_) => return redirect_to_dashboard().into_response(),
                Err(error) => {
                    log::debug!("Failed to save new user signup... oh no! {error:?}");
                    // TODO: throw an error page on this one
                    return redirect_to_home().into_response();
                }
            };
        }
    }

}

pub fn new() -> Router {
    let mut secret: [u8; 64] = [0; 64];
    rand::thread_rng().fill(&mut secret);

    Router::new()
        .route("/login", get(login))
        .route("/logout", get(logout))
        .route("/signup", post(signup))
}

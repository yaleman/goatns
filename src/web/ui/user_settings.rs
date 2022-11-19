use std::fmt::Display;

use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher};
use askama::Template;
use axum::response::{Html, Redirect};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use axum_macros::debug_handler;
use axum_sessions::extractors::WritableSession;
use chrono::{DateTime, Duration, Utc};
use enum_iterator::Sequence;
use oauth2::CsrfToken;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

use crate::web::SharedState;

static SESSION_CSRFTOKEN_FIELD: &str = "api_token_csrf_token";

#[derive(Template)]
#[template(path = "user_settings.html")]
struct Settings;

/// The user settings page at /ui/settings
pub async fn settings(Extension(_state): Extension<SharedState>) -> Html<String> {
    let context = Settings;

    Html::from(context.render().unwrap())
}

#[derive(Template)]
#[template(path = "user_api_tokens.html")]
struct ApiTokensGetPage {
    csrftoken: String,
    token_details: Vec<String>,
}

pub fn validate_csrf_expiry(user_input: String, session: &mut WritableSession) -> bool {
    let session_token: String = match session.get(SESSION_CSRFTOKEN_FIELD) {
        None => {
            session.remove(SESSION_CSRFTOKEN_FIELD);
            log::debug!("Couldn't get session token from storage");
            return false;
        }
        Some(value) => value,
    };

    let mut split = session_token.split('|');
    let csrf_token = match split.next() {
        None => {
            log::debug!("Didn't get split token");
            return false;
        }
        Some(value) => value,
    };

    if user_input != *csrf_token {
        log::debug!("Session and form CSRF token failed to match! {session_token} <> {csrf_token}");
        return false;
    }

    let expiry = match split.next() {
        None => {
            log::debug!("Couldn't get timestamp from stored CSRF Token");
            session.remove(SESSION_CSRFTOKEN_FIELD);
            return false;
        }
        Some(value) => value,
    };

    let expiry: DateTime<Utc> = match DateTime::parse_from_rfc3339(expiry) {
        Err(error) => {
            log::debug!("Failed to parse {expiry:?} into datetime: {error:?}");
            session.remove(SESSION_CSRFTOKEN_FIELD);
            return false;
        }
        Ok(value) => value.into(),
    };
    let now = Utc::now();

    if expiry < now {
        log::debug!("Token has expired at {expiry:?}, time is now {now:?}");
        session.remove(SESSION_CSRFTOKEN_FIELD);
        return false;
    }
    log::debug!("CSRF Token was valid!");
    true
}

/// Store a CSRF token with an expiry in the session store
///
/// Expiry defaults to 5 (minutes)
fn store_api_csrf_token(
    session: &mut WritableSession,
    expiry_plus_seconds: Option<i64>,
) -> Result<String, String> {
    let csrftoken = CsrfToken::new_random();
    let csrftoken = csrftoken.secret().to_string();

    let csrf_expiry: DateTime<Utc> =
        Utc::now() + Duration::seconds(expiry_plus_seconds.unwrap_or(300));

    let stored_csrf = format!("{csrftoken}|{}", csrf_expiry.to_rfc3339());

    // store it in the database
    if let Err(error) = session.insert(SESSION_CSRFTOKEN_FIELD, stored_csrf) {
        // TOOD: nice errors are nice but secure errors are better
        return Err(format!("Failed to store CSRF Token for user: {error:?}"));
    };
    Ok(csrftoken)
}

/// The user settings page at /ui/settings
pub async fn api_tokens_get(
    Extension(_state): Extension<SharedState>,
    mut session: WritableSession,
) -> Result<Html<String>, String> {
    let csrftoken = match store_api_csrf_token(&mut session, None) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };

    let context = ApiTokensGetPage {
        csrftoken,
        token_details: vec![
            "If the user had tokens this would be here...".to_string(),
            "Or maybe here?".to_string(),
        ],
    };

    Ok(Html::from(context.render().unwrap()))
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Sequence)]
pub enum ApiTokenLifetime {
    EightHours,
    TwentyFourHours,
    ThirtyDays,
    Forever,
}

impl From<ApiTokenLifetime> for i32 {
    fn from(input: ApiTokenLifetime) -> Self {
        match input {
            ApiTokenLifetime::EightHours => 8 * 60 * 60,
            ApiTokenLifetime::TwentyFourHours => 24 * 60 * 60,
            ApiTokenLifetime::ThirtyDays => 30 * 86400,
            ApiTokenLifetime::Forever => -1,
        }
    }
}

impl Display for ApiTokenLifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ApiTokenLifetime::EightHours => "Eight Hours (8h)",
            ApiTokenLifetime::TwentyFourHours => "Twenty-Four Hours (24h)",
            ApiTokenLifetime::ThirtyDays => "Thirty Days (30d)",
            ApiTokenLifetime::Forever => "No Expiry",
        })
    }
}

impl ApiTokenLifetime {
    fn variant_str(&self) -> String {
        match self {
            ApiTokenLifetime::EightHours => "EightHours".to_string(),
            ApiTokenLifetime::TwentyFourHours => "TwentyFourHours".to_string(),
            ApiTokenLifetime::ThirtyDays => "ThirtyDays".to_string(),
            ApiTokenLifetime::Forever => "Forever".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ApiTokenPageState {
    Start,
    Generating,
    Showing,
    Finished,
    Error,
}

impl Display for ApiTokenPageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiTokenPageState::Start => f.write_fmt(format_args!("Start")),
            ApiTokenPageState::Generating => f.write_fmt(format_args!("Generating")),
            ApiTokenPageState::Showing => f.write_fmt(format_args!("Showing")),
            ApiTokenPageState::Finished => f.write_fmt(format_args!("Finished")),
            ApiTokenPageState::Error => f.write_fmt(format_args!("Error")),
        }
    }
}

impl Default for ApiTokenPageState {
    fn default() -> Self {
        Self::Start
    }
}

impl ApiTokenPageState {
    pub fn next(&self) -> Self {
        match &self {
            ApiTokenPageState::Start => Self::Generating,
            ApiTokenPageState::Generating => Self::Showing,
            ApiTokenPageState::Showing => Self::Finished,
            ApiTokenPageState::Finished => Self::Error,
            ApiTokenPageState::Error => Self::Start,
        }
    }
}

/// Form handler for the api tokens post endpoint
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Template, Default)]
#[template(path = "user_api_token_form.html")]
pub struct ApiTokenPage {
    pub state: ApiTokenPageState,
    pub csrftoken: String,
    pub lifetime: Option<ApiTokenLifetime>,
    // TODO: this could be an empty vec, but the template is weird.
    pub lifetimes: Option<Vec<(String, String)>>,
}

/// The user settings page at /ui/settings
#[debug_handler]
pub async fn api_tokens_post(
    Extension(state): Extension<SharedState>,
    Form(form): Form<ApiTokenPage>,
    mut session: WritableSession,
) -> Result<Html<String>, Redirect> {
    eprintln!("Got form: {form:?}");

    if !validate_csrf_expiry(form.csrftoken, &mut session) {
        // TODO: redirect to the start
        log::debug!("Failed to validate csrf expiry");
        todo!();
    }

    let context = match form.state {
        // Present the "select a lifetime" page
        ApiTokenPageState::Start => {
            // present the user with the lifetime form
            let lifetimes: Vec<(String, String)> = enum_iterator::all::<ApiTokenLifetime>()
                .into_iter()
                .map(|l| (l.to_string(), l.variant_str()))
                .collect();
            eprintln!("Lifetimes: {lifetimes:?}");
            ApiTokenPage {
                csrftoken: store_api_csrf_token(&mut session, None).unwrap(),
                state: ApiTokenPageState::Start,
                lifetimes: Some(lifetimes),
                lifetime: None,
            }
        }
        // The user has set a lifetime and we're generating a token
        ApiTokenPageState::Generating => {
            log::debug!("In the 'Generating' state");

            // generate the credential

            let cookie_token = state.read().await;
            let cookie_token = cookie_token.config.api_cookie_secret();
            let issue_time = Utc::now();
            let lifetime: i32 = form.lifetime.unwrap().into();
            // TODO: get the user id from the thing
            let userid: i64 = 1;

            let api_token_to_hash =
                format!("{cookie_token:?}-{userid:?}-{issue_time:?}-{lifetime:?}-");

            // TODO: is rand_core the thing we want to use for generating randomness?
            let salt = SaltString::generate(&mut OsRng);
            // Argon2 with default params (Argon2id v19)
            let argon2 = Argon2::default();
            let password_hash = argon2
                .hash_password(api_token_to_hash.as_bytes(), &salt)
                .unwrap();

            let password_hash_string = password_hash.to_string();

            log::debug!("lol password_hash: {}", password_hash_string);

            // This is where we're checking that what we saved can be validated
            match PasswordHash::new(&password_hash_string) {
                Ok(winning) => log::info!("Succeeded at validating hash: {winning:?}"),
                Err(failed) => log::error!("Failed to validate hash: {failed:?}"),
            };

            // store the token in the database

            // store the api token in the session store
            if let Err(error) = session.insert("new_api_token", password_hash_string) {
                log::error!("Failed to store new API token in the session, ruh roh? {error:?}");
                // TODO: we need to remove the newly created token from the database again...
            };

            // redirect the user to the display page
            let csrftoken = store_api_csrf_token(&mut session, Some(30)).unwrap();
            return Err(Redirect::to(&format!(
                "/ui/settings/api_tokens?state={csrftoken}"
            )));
        }
        // We're showing the token to the user
        ApiTokenPageState::Showing => todo!(),
        ApiTokenPageState::Finished => todo!(),
        ApiTokenPageState::Error => todo!(),
    };
    Ok(Html::from(context.render().unwrap()))

    // Html::from("Welcome to the api tokens page".to_string())
}

/// Build the router for user settings
pub fn router() -> Router {
    Router::new()
        .route("/", get(settings))
        .route("/api_tokens", get(api_tokens_get))
        .route("/api_tokens", post(api_tokens_post))
}

use crate::db::{DBEntity, User};
use crate::web::ui::check_logged_in;
use crate::web::utils::create_api_token;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{Html, Redirect};
use axum::routing::get;
use axum::routing::post;
use axum::{debug_handler, Form, Router};

use axum::http::Uri;
use chrono::{DateTime, TimeDelta, Utc};
use enum_iterator::Sequence;
use oauth2::CsrfToken;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use tracing::{error, info};

use crate::db::UserAuthToken;
use crate::web::GoatState;

static SESSION_CSRFTOKEN_FIELD: &str = "api_token_csrf_token";

#[derive(Template)]
#[template(path = "user_settings.html")]
struct Settings {
    pub user_is_admin: bool,
}

/// The user settings page at /ui/settings
pub async fn settings(State(_state): State<GoatState>) -> Html<String> {
    let context = Settings {
        user_is_admin: true,
    };

    Html::from(context.render().unwrap())
}

#[derive(Template)]
#[template(path = "user_api_tokens.html")]
struct ApiTokensGetPage {
    csrftoken: String,
    tokens: Vec<Arc<UserAuthToken>>,
    tokenkey: Option<String>,
    token_value: Option<String>,
    pub user_is_admin: bool,
}

pub async fn validate_csrf_expiry(user_input: &str, session: &mut Session) -> bool {
    let session_token: String = match session.get(SESSION_CSRFTOKEN_FIELD).await.unwrap() {
        None => {
            session
                .remove_value(SESSION_CSRFTOKEN_FIELD)
                .await
                .map_err(|err| {
                    error!("Failed to remove CSRF token from session: {err:?}");
                    false
                })
                .unwrap();
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

    if user_input != csrf_token {
        log::debug!("Session and form CSRF token failed to match! user={user_input} <> session={csrf_token}");
        return false;
    }

    let expiry = match split.next() {
        None => {
            log::debug!("Couldn't get timestamp from stored CSRF Token");
            session
                .remove_value(SESSION_CSRFTOKEN_FIELD)
                .await
                .map_err(|err| {
                    error!("Failed to remove CSRF token from session: {err:?}");
                    false
                })
                .unwrap();
            return false;
        }
        Some(value) => value,
    };

    let expiry: DateTime<Utc> = match DateTime::parse_from_rfc3339(expiry) {
        Err(error) => {
            log::debug!("Failed to parse {expiry:?} into datetime: {error:?}");
            session
                .remove_value(SESSION_CSRFTOKEN_FIELD)
                .await
                .map_err(|err| {
                    error!("Failed to remove CSRF token from session: {err:?}");
                    false
                })
                .unwrap();
            return false;
        }
        Ok(value) => value.into(),
    };
    let now = Utc::now();

    if expiry < now {
        log::debug!("Token has expired at {expiry:?}, time is now {now:?}");
        session
            .remove_value(SESSION_CSRFTOKEN_FIELD)
            .await
            .map_err(|err| {
                error!("Failed to remove CSRF token from session: {err:?}");
                false
            })
            .unwrap();
        return false;
    }
    log::debug!("CSRF Token was valid!");
    true
}

/// Store a CSRF token with an expiry in the session store
///
/// Expiry defaults to 5 (minutes)
async fn store_api_csrf_token(
    session: &mut Session,
    expiry_plus_seconds: Option<i64>,
) -> Result<String, String> {
    let csrftoken = CsrfToken::new_random();
    let csrftoken = csrftoken.secret().to_string();

    let delta_time = match TimeDelta::try_seconds(expiry_plus_seconds.unwrap_or(300)) {
        Some(val) => val,
        None => return Err("Failed to calculate CSRF expiry!".to_string()),
    };
    let csrf_expiry: DateTime<Utc> = Utc::now() + delta_time;

    let stored_csrf = format!("{csrftoken}|{}", csrf_expiry.to_rfc3339());

    // store it in the session storage
    if let Err(error) = session.insert(SESSION_CSRFTOKEN_FIELD, stored_csrf).await {
        // TODO: nice errors are nice but secure errors are better
        return Err(format!("Failed to store CSRF Token for user: {error:?}"));
    };
    Ok(csrftoken)
}

/// The user settings page at /ui/settings/api_tokens
#[debug_handler]
pub async fn api_tokens_get(
    State(state): State<GoatState>,
    mut session: Session,
) -> Result<Html<String>, Redirect> {
    let user = check_logged_in(&mut session, Uri::from_static(URI_SETTINGS_API_TOKENS)).await?;

    let csrftoken = match store_api_csrf_token(&mut session, None).await {
        Ok(value) => value,
        Err(error) => {
            log::error!("Failed to store csrf token in DB: {error:?}");
            return Err(Redirect::to("/"));
        }
    };

    // pull token from the session store new_api_token
    let token_value: Option<String> = session.remove("new_api_token").await.unwrap();

    let tokenkey: Option<String> = session.remove("new_api_tokenkey").await.unwrap();

    let tokens =
        match UserAuthToken::get_all_user(&state.read().await.connpool, user.id.unwrap()).await {
            Err(error) => {
                log::error!("Failed to pull tokens for user {:#?}: {error:?}", user.id);
                vec![]
            }
            Ok(val) => val,
        };

    let context = ApiTokensGetPage {
        csrftoken,
        tokens,
        tokenkey,
        token_value,
        user_is_admin: user.admin,
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

/// WHere are they in their token-creation-lifecycle?
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ApiTokenCreatePageState {
    Start,
    Generating,
    Finished,
    Error,
}

impl Display for ApiTokenCreatePageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiTokenCreatePageState::Start => f.write_fmt(format_args!("Start")),
            ApiTokenCreatePageState::Generating => f.write_fmt(format_args!("Generating")),
            ApiTokenCreatePageState::Finished => f.write_fmt(format_args!("Finished")),
            ApiTokenCreatePageState::Error => f.write_fmt(format_args!("Error")),
        }
    }
}

impl Default for ApiTokenCreatePageState {
    fn default() -> Self {
        Self::Start
    }
}

impl ApiTokenCreatePageState {
    pub fn next(&self) -> Self {
        match &self {
            ApiTokenCreatePageState::Start => Self::Generating,
            ApiTokenCreatePageState::Generating => Self::Finished,
            ApiTokenCreatePageState::Finished => Self::Error,
            ApiTokenCreatePageState::Error => Self::Start,
        }
    }
}

/// Form handler for the api tokens post endpoint
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Default)]
pub struct ApiTokenForm {
    pub token_name: Option<String>,
    pub tokenkey: Option<String>,
    /// For when we want show the token
    pub token_value: Option<String>,
    pub csrftoken: String,
    pub state: ApiTokenCreatePageState,
    /// When the user selects the lifetime in the creation form
    pub lifetime: Option<ApiTokenLifetime>,
}
/// Form handler for the api tokens post endpoint
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Template, Default)]
#[template(path = "user_api_token_form.html")]
pub struct ApiTokenPage {
    pub token_name: Option<String>,
    pub tokenkey: Option<String>,
    /// For when we want show the token
    pub token_value: Option<String>,
    pub csrftoken: String,
    pub state: ApiTokenCreatePageState,
    /// When the user selects the lifetime in the creation form
    pub lifetime: Option<ApiTokenLifetime>,
    /// Used to show the possible lifetimes in the creation form
    pub lifetimes: Option<Vec<(String, String)>>,
    pub user_is_admin: bool,
}

/// The user settings page at /ui/settings
// #[debug_handler]
pub async fn api_tokens_post(
    mut session: Session,
    State(state): State<GoatState>,
    Form(form): Form<ApiTokenForm>,
) -> Result<Html<String>, Redirect> {
    eprintln!("Got form: {form:?}");

    let user = check_logged_in(&mut session, Uri::from_static(URI_SETTINGS_API_TOKENS)).await?;

    if !validate_csrf_expiry(&form.csrftoken, &mut session).await {
        // TODO: redirect to the start
        log::debug!("Failed to validate csrf expiry");
        return Err(Redirect::to("/ui/settings"));
    }

    let context = match form.state {
        // Present the "select a lifetime" page
        ApiTokenCreatePageState::Start => {
            // present the user with the lifetime form
            let lifetimes: Vec<(String, String)> = enum_iterator::all::<ApiTokenLifetime>()
                .map(|l| (l.to_string(), l.variant_str()))
                .collect();
            eprintln!("Lifetimes: {lifetimes:?}");
            ApiTokenPage {
                csrftoken: store_api_csrf_token(&mut session, None).await.unwrap(),
                state: ApiTokenCreatePageState::Start,
                token_name: None,
                lifetimes: Some(lifetimes),
                lifetime: None,
                tokenkey: None,
                token_value: None,
                user_is_admin: user.admin,
            }
        }
        // The user has set a lifetime and we're generating a token
        ApiTokenCreatePageState::Generating => {
            log::debug!("In the 'Generating' state");

            // generate the credential

            let state_reader = state.read().await;
            let api_cookie_secret = state_reader.config.api_cookie_secret();
            let lifetime: i32 = form.lifetime.unwrap().into();
            // get the user id from the session store, we should be able to safely unwrap here because we checked they were logged in up higher
            let user: User = match session.get("user").await.unwrap() {
                Some(val) => val,
                None => {
                    log::debug!("Couldn't get user from session store");
                    return Err(Redirect::to("/ui"));
                }
            };
            let userid: i64 = user.id.unwrap();

            let api_token = create_api_token(api_cookie_secret, lifetime, userid);

            // store the token in the database
            log::trace!("Starting to store token in the DB, grabbing writer...");
            println!("got writer...");
            let uat = UserAuthToken {
                id: None,
                name: form.token_name.unwrap(),
                issued: api_token.issued,
                expiry: api_token.expiry,
                userid,
                tokenkey: api_token.token_key.to_owned(),
                tokenhash: api_token.token_hash,
            };
            log::trace!("Starting to store token in the DB, grabbing transaction...");

            let mut txn = match state_reader.connpool.begin().await {
                Ok(val) => val,
                Err(error) => {
                    error!("Failed to pick up a txn for api token storage: {error:?}");
                    return Err(Redirect::to("/ui/settings/api_tokens"));
                }
            };

            log::trace!("Starting to store token in the DB, saving...");
            match uat.save_with_txn(&mut txn).await {
                Err(error) => {
                    error!("Failed to save api_token for user {:?}", error);
                    return Err(Redirect::to("/ui/settings/api_tokens"));
                }
                Ok(_) => {
                    // store the api token in the session store
                    if let Err(error) = session
                        .insert("new_api_token", &api_token.token_secret)
                        .await
                    {
                        log::error!(
                            "Failed to store new API token in the session, ruh roh? {error:?}"
                        );
                        txn.rollback().await.map_err(|e| {
                            log::error!(
                                "Txn rollback fail after failing to store the token: {e:?}"
                            );
                            Redirect::to("/ui/settings/api_tokens")
                        })?;
                        return Err(Redirect::to("/ui/settings/api_tokens"));
                    };

                    if let Err(error) = session
                        .insert("new_api_tokenkey", &api_token.token_key)
                        .await
                    {
                        log::error!(
                            "Failed to store new API tokenkey in the session, ruh roh? {error:?}"
                        );
                        txn.rollback().await.map_err(|e| {
                            log::error!("Txn rollback fail: {e:?}");
                            Redirect::to("/ui/settings/api_tokens")
                        })?;
                        // TODO: bail, which should roll back the txn
                        error!(
                            "Failed to store new API tokenkey in the session, ruh roh? {error:?}"
                        );
                        return Err(Redirect::to("/ui/settings/api_tokens"));
                    };

                    if let Err(error) = txn.commit().await {
                        log::error!(
                            "Failed to save the API token to storage, oh no? {:?}",
                            error
                        );
                        return Err(Redirect::to("/ui/settings/api_tokens"));
                    };
                }
            };
            // redirect the user to the display page
            let csrftoken = store_api_csrf_token(&mut session, Some(30)).await.unwrap();
            return Err(Redirect::to(&format!(
                "/ui/settings/api_tokens?state={csrftoken}?token_created=1"
            )));
        }

        ApiTokenCreatePageState::Finished => {
            info!("Created token, redirecting to the homepage");
            return Err(Redirect::to("/ui/settings/api_tokens"));
        }
        ApiTokenCreatePageState::Error => {
            error!("Got an error state in the form, redirecting to the start");
            return Err(Redirect::to("/ui/settings/api_tokens"));
        }
    };
    Ok(Html::from(context.render().unwrap()))

    // Html::from("Welcome to the api tokens page".to_string())
}

#[derive(Deserialize, Serialize, Debug, Clone, Template)]
#[template(path = "user_api_token_delete.html")]
pub struct ApiTokenDeleteTemplate {
    pub id: i64,
    pub token_name: Option<String>,
    pub csrftoken: String,
    pub user_is_admin: bool,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ApiTokenDeleteForm {
    pub id: i64,
    pub token_name: Option<String>,
    pub csrftoken: String,
}

const URI_SETTINGS_API_TOKENS: &str = "/ui/settings/api_tokens";

// #[debug_handler]
pub async fn api_tokens_delete_get(
    axum::extract::State(state): axum::extract::State<GoatState>,
    // Form(form): Form<ApiTokenPage>,
    Path(id): Path<String>,
    mut session: Session,
) -> Result<Html<String>, Redirect> {
    let user = check_logged_in(&mut session, Uri::from_static(URI_SETTINGS_API_TOKENS)).await?;

    let csrftoken = match store_api_csrf_token(&mut session, None).await {
        Ok(val) => val,
        Err(err) => {
            log::error!("Failed to store CSRF token in the session store: {err:?}");
            return Err(Redirect::to(URI_SETTINGS_API_TOKENS));
        }
    };

    let id = match i64::from_str(&id) {
        Ok(val) => val,
        Err(error) => {
            log::debug!("Got an invalid id parsing the URL: {error:?}");
            return Err(Redirect::to("/"));
        }
    };

    let state_reader = state.read().await;
    let pool = state_reader.connpool.clone();
    let uat = match UserAuthToken::get(&pool, id).await {
        Err(err) => {
            log::debug!("Requested delete for token: {err:?}");
            return Err(Redirect::to(URI_SETTINGS_API_TOKENS));
        }
        Ok(res) => {
            if res.userid != user.id.unwrap() {
                log::debug!(
                    "You can't delete another user's tokens! uid={} token.userid={}",
                    user.id.unwrap(),
                    res.userid
                );
                return Err(Redirect::to(URI_SETTINGS_API_TOKENS));
            };

            res
        }
    };

    let context = ApiTokenDeleteTemplate {
        id,
        token_name: Some(uat.name.clone()),
        csrftoken,
        user_is_admin: user.admin,
    };

    Ok(Html::from(context.render().unwrap()))
}

// #[debug_handler]
pub async fn api_tokens_delete_post(
    State(state): State<GoatState>,
    mut session: Session,
    Form(form): Form<ApiTokenDeleteForm>,
) -> Result<Html<String>, Redirect> {
    check_logged_in(&mut session, Uri::from_static(URI_SETTINGS_API_TOKENS)).await?;

    if !validate_csrf_expiry(&form.csrftoken, &mut session).await {
        // TODO: redirect to the start
        log::debug!("Failed to validate csrf expiry");
        return Err(Redirect::to("/ui/settings"));
    }

    log::debug!("Deleting token from Form: {form:?}");
    let user: User = match session.get("user").await.unwrap() {
        Some(val) => val,
        None => {
            log::debug!("Couldn't get user from session store");
            return Err(Redirect::to("/ui"));
        }
    };
    let pool = state.read().await.connpool.clone();
    let uat = match UserAuthToken::get(&pool, form.id).await {
        Err(err) => {
            log::debug!("Requested delete for existing token: {err:?}");
            return Err(Redirect::to(URI_SETTINGS_API_TOKENS));
        }
        Ok(res) => {
            if res.userid != user.id.unwrap() {
                log::debug!(
                    "You can't delete another user's tokens! uid={} token.userid={}",
                    user.id.unwrap(),
                    res.userid
                );
                return Err(Redirect::to(URI_SETTINGS_API_TOKENS));
            };

            res
        }
    };

    if let Err(error) = uat.delete(&pool).await {
        log::debug!("Failed to delete token {:?}: {error:?}", uat.id);
    };

    log::info!(
        "id={} action=api_token_delete token_id={}",
        uat.userid,
        uat.id.unwrap()
    );
    Err(Redirect::to(URI_SETTINGS_API_TOKENS))
}
/// Build the router for user settings
pub fn router() -> Router<GoatState> {
    Router::new()
        .route("/", get(settings))
        .route("/api_tokens", get(api_tokens_get))
        .route("/api_tokens", post(api_tokens_post))
        .route("/api_tokens/delete/:id", get(api_tokens_delete_get))
        .route("/api_tokens/delete/:id", post(api_tokens_delete_post))
}

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
use axum::{Form, Router};
use axum_macros::debug_handler;
// use axum_macros::debug_handler;
use axum_sessions::extractors::WritableSession;
use chrono::{DateTime, Duration, Utc};
use enum_iterator::Sequence;
use http::Uri;
use oauth2::CsrfToken;
use serde::{Deserialize, Serialize};

use crate::db::UserAuthToken;
use crate::web::GoatState;

static SESSION_CSRFTOKEN_FIELD: &str = "api_token_csrf_token";

#[derive(Template)]
#[template(path = "user_settings.html")]
struct Settings{
    pub user_is_admin: bool,
}

/// The user settings page at /ui/settings
pub async fn settings(State(_state): State<GoatState>) -> Html<String> {
    let context = Settings { user_is_admin: true };

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

pub fn validate_csrf_expiry(user_input: &str, session: &mut WritableSession) -> bool {
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

    if user_input != csrf_token {
        log::debug!("Session and form CSRF token failed to match! user={user_input} <> session={csrf_token}");
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

    // store it in the session storage
    if let Err(error) = session.insert(SESSION_CSRFTOKEN_FIELD, stored_csrf) {
        // TODO: nice errors are nice but secure errors are better
        return Err(format!("Failed to store CSRF Token for user: {error:?}"));
    };
    Ok(csrftoken)
}

/// The user settings page at /ui/settings/api_tokens
#[debug_handler]
pub async fn api_tokens_get(
    mut session: WritableSession,
    State(state): State<GoatState>,
) -> Result<Html<String>, Redirect> {

    let user = check_logged_in(&mut session, Uri::from_str("/ui/settings/api_tokens").unwrap()).await?;

    let csrftoken = match store_api_csrf_token(&mut session, None) {
        Ok(value) => value,
        Err(error) => {
            log::error!("Failed to store csrf token in DB: {error:?}");
            return Err(Redirect::to("/"));
        }
    };

    // pull token from the session store new_api_token
    let token_value: Option<String> = session.get("new_api_token");
    session.remove("new_api_token");

    let tokenkey: Option<String> = session.get("new_api_tokenkey");
    session.remove("new_api_tokenkey");

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
    mut session: WritableSession,
    State(state): State<GoatState>,
    Form(form): Form<ApiTokenPage>,
) -> Result<Html<String>, Redirect> {
    eprintln!("Got form: {form:?}");

    let user = check_logged_in(
        &mut session,
        Uri::from_str("/ui/settings/api_tokens").unwrap(),
    )
    .await?;

    if !validate_csrf_expiry(&form.csrftoken, &mut session) {
        // TODO: redirect to the start
        log::debug!("Failed to validate csrf expiry");
        return Err(Redirect::to("/ui/settings"));
    }

    let context = match form.state {
        // Present the "select a lifetime" page
        ApiTokenCreatePageState::Start => {
            // present the user with the lifetime form
            let lifetimes: Vec<(String, String)> = enum_iterator::all::<ApiTokenLifetime>()
                .into_iter()
                .map(|l| (l.to_string(), l.variant_str()))
                .collect();
            eprintln!("Lifetimes: {lifetimes:?}");
            ApiTokenPage {
                csrftoken: store_api_csrf_token(&mut session, None).unwrap(),
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
            let user: User = session.get("user").unwrap();
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
                Err(error) => todo!(
                    "Need to handle failing to pick up a txn for api token storage: {error:?}"
                ),
            };

            log::trace!("Starting to store token in the DB, saving...");
            match uat.save_with_txn(&mut txn).await {
                Err(error) => todo!("Need to handle this! {error:?}"),
                Ok(_) => {
                    // store the api token in the session store
                    if let Err(error) = session.insert("new_api_token", &api_token.token_secret) {
                        log::error!(
                            "Failed to store new API token in the session, ruh roh? {error:?}"
                        );
                        txn.rollback()
                            .await
                            .map_err(|e| {
                                log::error!("Txn rollback fail: {e:?}");
                                todo!()
                            })
                            .unwrap();
                        // TODO: bail, which should roll back the txn
                        todo!("Failed to store new API token in the session, ruh roh? {error:?}");
                    };

                    if let Err(error) = session.insert("new_api_tokenkey", &api_token.token_key) {
                        log::error!(
                            "Failed to store new API tokenkey in the session, ruh roh? {error:?}"
                        );
                        txn.rollback()
                            .await
                            .map_err(|e| {
                                log::error!("Txn rollback fail: {e:?}");
                                todo!()
                            })
                            .unwrap();
                        // TODO: bail, which should roll back the txn
                        todo!(
                            "Failed to store new API tokenkey in the session, ruh roh? {error:?}"
                        );
                    };

                    if let Err(error) = txn.commit().await {
                        log::error!("Failed to save the API token to storage, oh no?");
                        todo!("Failed to save the API token to storage, oh no? {error:?}");
                    };
                }
            };
            // redirect the user to the display page
            let csrftoken = store_api_csrf_token(&mut session, Some(30)).unwrap();
            return Err(Redirect::to(&format!(
                "/ui/settings/api_tokens?state={csrftoken}?token_created=1"
            )));
        }

        ApiTokenCreatePageState::Finished => todo!(),
        ApiTokenCreatePageState::Error => todo!(),
    };
    Ok(Html::from(context.render().unwrap()))

    // Html::from("Welcome to the api tokens page".to_string())
}

#[derive(Deserialize, Serialize, Debug, Clone, Template)]
#[template(path = "user_api_token_delete.html")]
pub struct ApiTokenDelete {
    pub id: i64,
    pub token_name: Option<String>,
    pub csrftoken: String,
    pub user_is_admin: bool,
}

// #[debug_handler]
pub async fn api_tokens_delete_get(
    axum::extract::State(state): axum::extract::State<GoatState>,
    // Form(form): Form<ApiTokenPage>,
    Path(id): Path<String>,
    mut session: WritableSession,
) -> Result<Html<String>, Redirect> {
    let user = check_logged_in(
        &mut session,
        Uri::from_str("/ui/settings/api_tokens").unwrap(),
    )
    .await?;

    let csrftoken = match store_api_csrf_token(&mut session, None) {
        Ok(val) => val,
        Err(err) => {
            log::error!("Failed to store CSRF token in the session store: {err:?}");
            return Err(Redirect::to("/ui/settings/api_tokens"));
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
            return Err(Redirect::to("/ui/settings/api_tokens"));
        }
        Ok(res) => {
            if res.userid != user.id.unwrap() {
                log::debug!(
                    "You can't delete another user's tokens! uid={} token.userid={}",
                    user.id.unwrap(),
                    res.userid
                );
                return Err(Redirect::to("/ui/settings/api_tokens"));
            };

            res
        }
    };

    let context = ApiTokenDelete {
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
    mut session: WritableSession,
    Form(form): Form<ApiTokenDelete>,
) -> Result<Html<String>, Redirect> {
    check_logged_in(
        &mut session,
        Uri::from_str("/ui/settings/api_tokens").unwrap(),
    )
    .await?;

    if !validate_csrf_expiry(&form.csrftoken, &mut session) {
        // TODO: redirect to the start
        log::debug!("Failed to validate csrf expiry");
        return Err(Redirect::to("/ui/settings"));
    }

    log::debug!("Deleting token from Form: {form:?}");
    let user: User = session.get("user").unwrap();
    let pool = state.read().await.connpool.clone();
    let uat = match UserAuthToken::get(&pool, form.id).await {
        Err(err) => {
            log::debug!("Requested delete for existing token: {err:?}");
            return Err(Redirect::to("/ui/settings/api_tokens"));
        }
        Ok(res) => {
            if res.userid != user.id.unwrap() {
                log::debug!(
                    "You can't delete another user's tokens! uid={} token.userid={}",
                    user.id.unwrap(),
                    res.userid
                );
                return Err(Redirect::to("/ui/settings/api_tokens"));
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
    Err(Redirect::to("/ui/settings/api_tokens"))
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

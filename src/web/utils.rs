use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::debug_handler;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use chrono::{DateTime, TimeDelta, Utc};
use rand::distributions::{Alphanumeric, DistString};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

use crate::db::TokenSearchRow;

pub fn redirect_to_home() -> Redirect {
    Redirect::to("/")
}

pub fn redirect_to_login() -> Redirect {
    Redirect::to("/auth/login")
}

pub fn redirect_to_dashboard() -> Redirect {
    Redirect::to("/ui")
}
pub fn redirect_to_zones_list() -> Redirect {
    Redirect::to("/ui/zones/list")
}

#[derive(Debug, Clone)]
pub struct ApiToken {
    /// The username
    pub token_key: String,
    /// The password
    pub token_secret: String,
    /// This goes into the database
    pub token_hash: String,
    /// Guess?
    pub issued: DateTime<Utc>,
    /// For a time or forever!
    pub expiry: Option<DateTime<Utc>>,
}

pub fn create_api_token(api_cookie_secret: &[u8], lifetime: i32, userid: i64) -> ApiToken {
    let issued = Utc::now();
    let expiry = match lifetime {
        -1 => None,
        _ => Some(
            issued + TimeDelta::try_seconds(lifetime.into()).expect("Failed to calculate seconds!"),
        ),
    };
    let api_token_to_hash = format!("{api_cookie_secret:?}-{userid:?}-{issued:?}-{lifetime:?}-");

    let api_token = hex::encode(Sha256::digest(api_token_to_hash));
    let token_secret = format!("goatns_{api_token}");
    log::trace!("Final token: {token_secret}");

    // TODO: is rand_core the thing we want to use for generating randomness?
    let salt = SaltString::generate(&mut OsRng);

    log::debug!("generating hash");
    // Argon2 with default params (Argon2id v19)
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(token_secret.as_bytes(), &salt)
        .unwrap();

    let password_hash_string = password_hash.to_string();
    log::debug!("done");

    let token_key = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);
    let token_key = format!("GA{}", token_key);
    ApiToken {
        token_key,
        token_secret,
        token_hash: password_hash_string,
        issued,
        expiry,
    }
}

/// validate an API token matches our thingamajig
pub fn validate_api_token(token: &TokenSearchRow, payload_token: &str) -> Result<(), String> {
    let passwordhash =
        match argon2::PasswordHash::parse(&token.tokenhash, argon2::password_hash::Encoding::B64) {
            Ok(val) => {
                #[cfg(test)]
                println!("Hashed payload: {val:?}");
                val
            }
            Err(err) => {
                return Err(format!(
                    "Failed to parse token ({:?}) into hash: {err:?}",
                    token.tokenhash
                ));
            }
        };
    Argon2::default()
        .verify_password(payload_token.as_bytes(), &passwordhash)
        .map_err(|e| format!("validation error: {e:?}"))
}

#[debug_handler]
pub async fn handler_404() -> impl IntoResponse {
    axum::response::Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-type", "text/html")
        .body(
            "<h1>Oh no!</h1><p>You've found a 404, try <a href='#' onclick='history.back();'>going back</a> or <a href='/'>home!</a></p>"
                .to_string(),
        )
        .unwrap()
        .into_response()
}

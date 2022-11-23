use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use axum::response::Redirect;
use chrono::{DateTime, Duration, Utc};
use rand::distributions::{Alphanumeric, DistString};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

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
    pub token_key: String,
    pub token_secret: String,
    pub token_hash: String,
    pub issued: DateTime<Utc>,
    pub expiry: Option<DateTime<Utc>>,
}

pub fn create_api_token(api_cookie_secret: &[u8], lifetime: i32, userid: i64) -> ApiToken {
    let issued = Utc::now();
    let expiry = match lifetime {
        -1 => None,
        _ => Some(issued + Duration::seconds(lifetime.into())),
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

    // TODO: Generate tokenkey
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

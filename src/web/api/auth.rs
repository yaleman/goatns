use std::str::from_utf8;

use argon2::{Argon2, PasswordVerifier, PasswordHash};
use axum::{Extension, Json};
use axum::middleware::Next;
use axum::response::Response;
use axum_macros::debug_handler;
use axum_sessions::extractors::WritableSession;
use http::{Request, StatusCode};
use serde::{Deserialize, Serialize};

use crate::db::User;
use crate::web::SharedState;


pub async fn check_auth<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {

    let auth_header = match req.headers().get("Authorization"){
        None => return Err(StatusCode::UNAUTHORIZED),
        Some(val) => from_utf8(val.as_bytes()).unwrap(),
    };

    if !auth_header.starts_with("Bearer") {
        return Err(StatusCode::BAD_REQUEST)
    }

    let auth_token = match auth_header.split(' ').nth(1) {
        Some(val) => val,
        None => return Err(StatusCode::BAD_REQUEST)
    };

    log::debug!("Got auth header with bearer token: {auth_token:?}");
    // wait for the middleware to come back
    Ok(next.run(req).await)
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub username: String,
    pub token: String,
}

#[derive(Debug, Deserialize,Serialize, Clone)]
pub struct AuthResponse {
    pub message: String
}


#[debug_handler]
pub async fn login(
    state: Extension<SharedState>,
    payload: Json<AuthPayload>,
    mut session: WritableSession,
) -> Result<Json<AuthResponse>,Json<AuthResponse>> {
    log::debug!("Got payload: {payload:?}");

    let pool = state.read().await;
    let mut pool = pool.connpool.clone();
    let tokens = match User::get_tokens_by_username(&mut pool, &payload.username).await {
        Ok(val) => val,
        Err(err) => {
            log::debug!("Failed to get rows for {}: {err:?}", payload.username);
            let resp = AuthResponse{message: "failed".to_string()};
            return Err(Json(resp));
        },
    };

    if tokens.is_empty() {
        session.destroy();
        log::info!("action=api_login user={} result=failure reason=\"no tokens found in database for user\"", payload.username);
        return Err(Json(AuthResponse{message: "failure".to_string()}));
    }

    for token in tokens {
        let passwordhash = match PasswordHash::parse(
            &token.tokenhash,
            argon2::password_hash::Encoding::B64) {
                Ok(val) => val,
                Err(err) => {
                    log::error!("Failed to parse token ({token:?}) from database: {err:?}");
                    continue
                }
            };
        if Argon2::default().verify_password(payload.token.as_bytes(), &passwordhash).is_ok() {


            let session_user = session.insert("user", &token.user);
            let session_authref = session.insert("authref", token.user.authref);
            let session_signin = session.insert("signed_in", true);

            if session_authref.is_err() | session_user.is_err() | session_signin.is_err() {
                session.destroy();
                log::error!("Failed to store fresh session for user");
                log::info!("action=api_login user={} result=failure", payload.username);
                return Err(Json(AuthResponse{message: "failure".to_string()}))
            };

            log::info!("action=api_login user={} result=success", payload.username);
            return Ok(Json(AuthResponse{message: "success".to_string()}))
        } else {
            log::debug!("Failed to validate token {:?} for user {}",token, payload.username);
        }
    }

    session.destroy();
    log::info!("action=api_login user={} result=failure reason=\"no tokens found\"", payload.username);
    return Err(Json(AuthResponse{message: "failure".to_string()}))
}


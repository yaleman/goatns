use std::str::from_utf8;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::middleware::Next;
use axum::response::Response;
use axum::{Extension, Json};
use axum_macros::debug_handler;
use axum_sessions::extractors::WritableSession;
use http::{Request, StatusCode};
use serde::{Deserialize, Serialize};

use crate::db::User;
use crate::web::SharedState;

pub async fn check_auth<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let auth_header = match req.headers().get("Authorization") {
        None => return Err(StatusCode::UNAUTHORIZED),
        Some(val) => from_utf8(val.as_bytes()).unwrap(),
    };

    if !auth_header.starts_with("Bearer") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let auth_token = match auth_header.split(' ').nth(1) {
        Some(val) => val,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    log::debug!("Got auth header with bearer token: {auth_token:?}");
    // wait for the middleware to come back
    Ok(next.run(req).await)
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub tokenkey: String,
    pub token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthResponse {
    pub message: String,
}

#[debug_handler]
pub async fn login(
    state: Extension<SharedState>,
    payload: Json<AuthPayload>,
    mut session: WritableSession,
) -> Result<Json<AuthResponse>, Json<AuthResponse>> {
    log::debug!("Got payload: {payload:?}");

    let pool = state.read().await;
    let mut pool = pool.connpool.clone();
    let token = match User::get_token(&mut pool, &payload.tokenkey).await {
        Ok(val) => val,
        Err(err) => {
            log::info!(
                "action=api_login tokenkey={} result=failure reason=\"no token found\"",
                payload.tokenkey
            );
            log::debug!("Error: {err:?}");
            session.destroy();
            let resp = AuthResponse {
                message: "token not found".to_string(),
            };
            return Err(Json(resp));
        }
    };

    let passwordhash = match PasswordHash::parse(
        &token.tokenhash,
        argon2::password_hash::Encoding::B64,
    ) {
        Ok(val) => val,
        Err(err) => {
            log::error!("Failed to parse token ({token:?}) from database: {err:?}");
            log::info!("action=api_login tokenkey={} result=failure reason=\"failed to parse token from dtatabase\"", payload.tokenkey);

            session.destroy();
            let resp = AuthResponse {
                message: "token error".to_string(),
            };
            return Err(Json(resp));
        }
    };
    match Argon2::default().verify_password(payload.token.as_bytes(), &passwordhash) {
        Ok(_) => {
            let session_user = session.insert("user", &token.user);
            let session_authref = session.insert("authref", token.user.authref);
            let session_signin = session.insert("signed_in", true);

            if session_authref.is_err() | session_user.is_err() | session_signin.is_err() {
                session.destroy();
                log::info!("action=api_login tokenkey={} result=failure reason=\"failed to store session for user\"", payload.tokenkey);
                return Err(Json(AuthResponse {
                    message: "system failure, please contact an admin".to_string(),
                }));
            };

            log::info!("action=api_login user={} result=success", payload.tokenkey);
            Ok(Json(AuthResponse {
                message: "success".to_string(),
            }))
        }
        Err(_) => {
            session.destroy();
            log::error!(
        "action=api_login username={} userid={} tokenkey=\"{:?}\" result=failure reason=\"failed to match token\"",
        token.user.username,
        token.user.id.unwrap(),
        payload.tokenkey,
    );
            Err(Json(AuthResponse {
                message: "system failure, please contact an admin".to_string(),
            }))
        }
    }
}

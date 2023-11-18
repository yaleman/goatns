// use std::str::from_utf8;

// use axum::middleware::Next;
// use axum::response::Response;
use axum::{extract::State, Json};
// use axum_macros::debug_handler;
use http::StatusCode;
// use http::{Request, StatusCode};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::db::User;
use crate::web::utils::validate_api_token;
use crate::web::GoatState;

#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub tokenkey: String,
    pub token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthResponse {
    pub message: String,
}

// #[debug_handler]
pub async fn login(
    State(state): State<GoatState>,
    session: Session,
    payload: Json<AuthPayload>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<AuthResponse>)> {
    #[cfg(test)]
    println!("Got login payload: {payload:?}");
    #[cfg(not(test))]
    log::debug!("Got login payload: {payload:?}");

    let mut pool = state.read().await.connpool.clone();
    let token = match User::get_token(&mut pool, &payload.tokenkey).await {
        Ok(val) => val,
        Err(err) => {
            #[cfg(test)]
            println!(
                "action=api_login tokenkey={} result=failure reason=\"no token found\"",
                payload.tokenkey
            );
            log::info!(
                "action=api_login tokenkey={} result=failure reason=\"no token found\"",
                payload.tokenkey
            );
            #[cfg(test)]
            println!("Error: {err:?}");
            log::debug!("Error: {err:?}");
            session.flush();
            let resp = AuthResponse {
                message: "token not found".to_string(),
            };
            return Err((StatusCode::UNAUTHORIZED, Json(resp)));
        }
    };
    match validate_api_token(&token, &payload.token) {
        Ok(_) => {
            println!("Successfully validated token on login");
            let session_user = session.insert("user", &token.user);
            let session_authref = session.insert("authref", token.user.authref);
            let session_signin = session.insert("signed_in", true);

            if session_authref.is_err() | session_user.is_err() | session_signin.is_err() {
                session.flush();
                log::info!("action=api_login tokenkey={} result=failure reason=\"failed to store session for user\"", payload.tokenkey);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuthResponse {
                        message: "system failure, please contact an admin".to_string(),
                    }),
                ));
            };
            #[cfg(test)]
            println!("action=api_login user={} result=success", payload.tokenkey);
            #[cfg(not(test))]
            log::info!("action=api_login user={} result=success", payload.tokenkey);
            Ok((
                StatusCode::OK,
                Json(AuthResponse {
                    message: "success".to_string(),
                }),
            ))
        }
        Err(err) => {
            session.flush();
            #[cfg(test)]
            println!("Failed to validate token! {err:?}");
            log::error!(
        "action=api_login username={} userid={} tokenkey=\"{:?}\" result=failure reason=\"failed to match token: {err:?}\"",
        token.user.username,
        token.user.id.unwrap(),
        payload.tokenkey,
        );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    message: "system failure, please contact an admin".to_string(),
                }),
            ))
        }
    }
}

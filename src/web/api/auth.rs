use axum::http::StatusCode;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use tracing::{debug, error, info};
use utoipa::ToSchema;

use crate::db::User;
use crate::web::utils::validate_api_token;
use crate::web::GoatState;

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct AuthPayload {
    pub token_key: String,
    pub token_secret: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct AuthResponse {
    pub message: String,
}

impl From<String> for AuthResponse {
    fn from(message: String) -> Self {
        AuthResponse { message }
    }
}

#[utoipa::path(
    post,
    path = "/api/login",
    operation_id = "login",
    request_body = AuthPayload,
    responses(
        (status = 200, description = "Login Successful"),
        (status = 403, description = "Auth failed"),
        (status = 500, description = "Something broke!"),
    ),
    tag = "Authentication",
)]
pub async fn login(
    State(state): State<GoatState>,
    session: Session,
    payload: Json<AuthPayload>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<AuthResponse>)> {
    #[cfg(test)]
    println!("Got login payload: {payload:?}");
    #[cfg(not(test))]
    debug!("Got login payload: {payload:?}");
    let mut pool = state.read().await.connpool.clone();
    let token = match User::get_token(&mut pool, &payload.token_key).await {
        Ok(val) => val,
        Err(err) => {
            info!(
                "action=api_login tokenkey={} result=failure reason=\"no token found\"",
                payload.token_key
            );
            debug!("Error: {err:?}");
            session.flush().await.map_err(|err| {
                error!("Failed to flush session: {err:?}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuthResponse::from("Failed to flush session!".to_string())),
                )
            })?;
            let resp = AuthResponse {
                message: "token not found".to_string(),
            };
            return Err((StatusCode::UNAUTHORIZED, Json(resp)));
        }
    };

    match validate_api_token(&token, &payload.token_secret) {
        Ok(_) => {
            println!("Successfully validated token on login");
            let session_user = session.insert("user", &token.user).await;
            let session_authref = session.insert("authref", token.user.authref).await;
            let session_signin = session.insert("signed_in", true).await;

            if session_authref.is_err() | session_user.is_err() | session_signin.is_err() {
                session.flush().await.map_err(|err| {
                    error!("Failed to flush session: {err:?}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AuthResponse::from("Failed to flush session!".to_string())),
                    )
                })?;
                info!("action=api_login tokenkey={} result=failure reason=\"failed to store session for user\"", payload.token_key);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuthResponse {
                        message: "system failure, please contact an admin".to_string(),
                    }),
                ));
            };
            info!("action=api_login user={} result=success", payload.token_key);
            Ok((
                StatusCode::OK,
                Json(AuthResponse {
                    message: "success".to_string(),
                }),
            ))
        }
        Err(err) => {
            session.flush().await.map_err(|err| {
                error!("Failed to flush session: {err:?}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuthResponse::from("Failed to flush session!".to_string())),
                )
            })?;
            #[cfg(test)]
            println!("Failed to validate token! {err:?}");
            error!(
        "action=api_login username={} userid={} tokenkey=\"{:?}\" result=failure reason=\"failed to match token: {err:?}\"",
        token.user.username,
        token.user.id.map(|id| id.to_string()).unwrap_or("<unknown user id>".to_string()),
        payload.token_key,
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

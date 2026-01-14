use axum::http::StatusCode;
use axum::{Json, extract::State};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use tracing::{debug, error, info};
use utoipa::ToSchema;

use crate::db::entities;
use crate::web::utils::validate_api_token;
use crate::web::{GoatState, GoatStateTrait};

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
pub async fn api_token_login(
    State(state): State<GoatState>,
    session: Session,
    payload: Json<AuthPayload>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<AuthResponse>)> {
    #[cfg(test)]
    println!("Got login payload: {payload:?}");
    #[cfg(not(test))]
    debug!("Got login payload: {payload:?}");
    let (token, user) = match entities::user_tokens::Entity::find()
        .filter(entities::user_tokens::Column::Key.eq(&payload.token_key))
        .find_also_related(entities::users::Entity)
        .one(&*state.connpool().await)
        .await
    {
        Ok(Some((token, Some(user)))) => (token, user),
        Ok(Some((_, None))) => {
            info!(
                "action=api_login tokenkey={} result=failure reason=\"no user found for token\"",
                payload.token_key
            );
            let resp = AuthResponse {
                message: "token not found".to_string(),
            };
            return Err((StatusCode::UNAUTHORIZED, Json(resp)));
        }
        Ok(None) => {
            info!(
                "action=api_login tokenkey={} result=failure reason=\"no token found\"",
                payload.token_key
            );
            let resp = AuthResponse {
                message: "token not found".to_string(),
            };
            return Err((StatusCode::UNAUTHORIZED, Json(resp)));
        }
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
            // TODO: work out what we want to store here
            let session_user = session.insert("user", &user.id).await;
            let session_authref = session.insert("authref", &user.authref).await;
            let session_signin = session.insert("signed_in", true).await;

            if session_authref.is_err() | session_user.is_err() | session_signin.is_err() {
                session.flush().await.map_err(|err| {
                    error!("Failed to flush session: {err:?}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AuthResponse::from("Failed to flush session!".to_string())),
                    )
                })?;
                info!(
                    "action=api_login tokenkey={} result=failure reason=\"failed to store session for user\"",
                    payload.token_key
                );
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
                user.username, user.id, payload.token_key,
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

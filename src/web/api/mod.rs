use super::*;
use crate::db::User;
use crate::zones::FileZone;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, post, put};
use axum::Json;
use tower_sessions::Session;
use tracing::debug;

use serde::Deserialize;
use serde::Serialize;

pub mod auth;
pub(crate) mod docs;
pub mod filezone;
pub mod filezonerecord;

#[macro_export]
/// message, status
macro_rules! error_result_json {
    ($msg:expr, $status:expr) => {
        Err(($status, Json(ErrorResult::from($msg))))
    };
}

#[derive(Serialize)]
pub struct NotImplemented {
    response: String,
}

impl Default for NotImplemented {
    fn default() -> Self {
        Self {
            response: "This endpoint is not yet implemented".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ErrorResult {
    #[allow(dead_code)]
    pub message: String,
}

impl From<&str> for ErrorResult {
    fn from(input: &str) -> Self {
        ErrorResult {
            message: input.to_string(),
        }
    }
}

/// This gets applied to DBEntities
// #[async_trait]
// trait APIEntity {
//     /// Save the entity to the database
//     async fn api_create(
//         State(state): State<GoatState>,
//         session: Session,
//         Json(payload): Json<serde_json::Value>,
//     ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)>;
//     /// HTTP Put <https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/PUT>
//     async fn api_update(
//         State(state): State<GoatState>,
//         session: Session,
//         Json(payload): Json<serde_json::Value>,
//     ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)>;
//     async fn api_get(
//         State(state): State<GoatState>,
//         session: Session,
//         Path(id): Path<i64>,
//     ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)>;

//     /// Delete an object
//     /// <https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/DELETE>
//     async fn api_delete(
//         State(state): State<GoatState>,
//         session: Session,
//         Path(id): Path<i64>,
//     ) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)>;
// }

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileZoneResponse {
    pub message: String,
    pub zone: Option<FileZone>,
    pub id: Option<i64>,
}

#[derive(Serialize)]
pub struct GoatNSVersion {
    version: String,
}
impl Default for GoatNSVersion {
    fn default() -> Self {
        Self {
            version: format!("GoatNS {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

pub async fn version_get() -> Json<GoatNSVersion> {
    Json::from(GoatNSVersion::default())
}

/// Check API authentication by extracting user from session
/// Returns the authenticated user or an error response
pub async fn check_api_auth(session: &Session) -> Result<User, (StatusCode, Json<ErrorResult>)> {
    match session.get("user").await {
        Ok(Some(user)) => Ok(user),
        Ok(None) => {
            #[cfg(test)]
            println!("User not found in API call");
            #[cfg(not(test))]
            debug!("User not found in API call");
            error_result_json!("", StatusCode::FORBIDDEN)
        }
        Err(err) => {
            #[cfg(test)]
            println!("Session error in API call: {err:?}");
            #[cfg(not(test))]
            debug!("Session error in API call: {err:?}");
            error_result_json!("Session error", StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn new() -> Router<GoatState> {
    Router::new()
        .route("/zone", post(filezone::api_create))
        .route("/zone", put(filezone::api_update))
        .route("/zone/{id}", get(filezone::api_get))
        .route("/zone/{id}", delete(filezone::api_delete))
        .route("/record", post(filezonerecord::api_create))
        .route("/record", put(filezonerecord::api_update))
        .route("/record/{id}", get(filezonerecord::api_get))
        .route("/record/{id}", delete(filezonerecord::api_delete))
        .route("/login", post(auth::login))
}

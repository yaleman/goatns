use axum::{Router, routing::get};
use prelude::*;
pub mod auth;
pub(crate) mod docs;
pub(crate) mod prelude;
pub mod records;
pub mod zones;

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

pub(crate) fn error_result_json(
    message: &str,
    status: StatusCode,
) -> (StatusCode, axum::Json<ErrorResult>) {
    (status, axum::Json(ErrorResult::from(message)))
}

/// Check API authentication by extracting user from session
/// Returns the authenticated user or an error response
pub async fn check_api_auth(
    session: &Session,
) -> Result<entities::users::Model, (StatusCode, Json<ErrorResult>)> {
    match session.get(SESSION_USER_KEY).await {
        Ok(Some(user)) => Ok(user),
        Ok(None) => {
            #[cfg(test)]
            error!("User not found in API call");
            #[cfg(not(test))]
            error!("User not found in API call");
            Err(error_result_json("", StatusCode::FORBIDDEN))
        }
        Err(err) => {
            error!("Session error in API call: {err:?}");
            Err(error_result_json(
                "Session error",
                StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

pub fn new() -> Router<GoatState> {
    Router::new()
        .route("/zone", post(zones::api_zone_create))
        .route("/zone", put(zones::api_zone_update))
        .route(
            "/zone/{zone_id}",
            get(zones::api_get).delete(zones::api_zone_delete),
        )
        .route("/record", post(records::api_record_create))
        .route("/record", put(records::api_record_update))
        .route(
            "/record/{record_id}",
            get(records::api_record_get).delete(records::api_record_delete),
        )
        .route("/login", post(auth::api_token_login))
}

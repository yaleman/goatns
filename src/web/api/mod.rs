use prelude::*;

use axum::{Router, routing::get};

pub mod auth;
pub(crate) mod docs;
pub mod filezone;
pub mod filezonerecord;
pub(crate) mod prelude;

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
        .route("/zone", post(filezone::api_zone_create))
        .route("/zone", put(filezone::api_zone_update))
        .route(
            "/zone/{zone_id}",
            get(filezone::api_get).delete(filezone::api_zone_delete),
        )
        .route("/record", post(filezonerecord::api_record_create))
        .route("/record", put(filezonerecord::api_record_update))
        .route(
            "/record/{record_id}",
            get(filezonerecord::api_record_get).delete(filezonerecord::api_record_delete),
        )
        .route("/login", post(auth::api_token_login))
}

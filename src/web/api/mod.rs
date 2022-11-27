use super::*;
use crate::zones::FileZone;
use crate::zones::FileZoneRecord;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::{delete, post, put};
use axum::Json;
use axum_sessions::extractors::ReadableSession;
use serde::Deserialize;
use serde::Serialize;

pub mod auth;
pub mod filezone;
pub mod filezonerecord;
pub use filezone::*;
pub use filezonerecord::*;

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
#[async_trait]
trait APIEntity {
    /// Save the entity to the database
    async fn api_create(
        State(state): State<GoatState>,
        session: ReadableSession,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)>;
    /// HTTP Put https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/PUT
    async fn api_update(
        State(state): State<GoatState>,
        session: ReadableSession,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)>;
    async fn api_get(
        State(state): State<GoatState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)>;

    /// Delete an object
    /// https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/DELETE
    async fn api_delete(
        State(state): State<GoatState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)>;
}

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

pub fn new() -> Router<GoatState> {
    Router::new()
        // just zone things
        .route("/zone", post(FileZone::api_create))
        .route("/zone", put(FileZone::api_update))
        .route("/zone/:id", get(FileZone::api_get))
        .route("/zone/:id", delete(FileZone::api_delete))
        .route("/record", post(FileZoneRecord::api_create))
        .route("/record", put(FileZoneRecord::api_update))
        .route("/record/:id", get(FileZoneRecord::api_get))
        .route("/record/:id", delete(FileZoneRecord::api_delete))
        .route("/login", post(auth::login))
}

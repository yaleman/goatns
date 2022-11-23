use super::*;
use crate::db::DBEntity;

use crate::db::User;
use crate::db::ZoneOwnership;
use crate::zones::FileZone;
use axum::extract::Extension;
use axum::extract::Path;
use axum::routing::{delete, post};
use axum::Json;
// use axum_macros::debug_handler;
use axum_sessions::extractors::ReadableSession;
use serde::Deserialize;
use serde::Serialize;

pub mod auth;

#[derive(Serialize)]
pub struct NotImplemented {
    response: String,
}

// enum APIResult {
//     PermissionDenied,
//     ItemNotFound,
// }

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

macro_rules! error_result_json {
    ($msg:expr, $status:expr) => {
        Err(($status, Json(ErrorResult::from($msg))))
    };
}

/// This gets applied to DBEntities
#[async_trait]
trait APIEntity {
    /// Save the
    async fn api_create(
        state: Extension<SharedState>,
        session: ReadableSession,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)>;
    // async fn api_update(pool: &Pool<Sqlite>, id: i64) -> Result<Json<String>, Json<ErrorResult>>;
    async fn api_get(
        state: Extension<SharedState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)>;

    /// Delete an object
    /// https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/DELETE
    async fn api_delete(
        state: Extension<SharedState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)>;
}

#[async_trait]
impl APIEntity for FileZone {
    async fn api_create(
        state: Extension<SharedState>,
        session: ReadableSession,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)> {
        log::debug!("Got api_create payload: {payload:?}");
        let user: User = match session.get("user") {
            Some(val) => val,
            None => {
                log::debug!("User not found in api_create call");
                return error_result_json!("", StatusCode::FORBIDDEN);
            }
        };

        let zone: FileZone = match serde_json::from_value(payload) {
            Ok(val) => val,
            Err(err) => {
                log::debug!("Failed to deser payload: {err:?}");
                let res = format!("Invalid payload: {err:?}");
                return error_result_json!(res.as_str(), StatusCode::BAD_REQUEST);
            }
        };

        // check to see if the zone exists
        let mut txn = state.connpool().await.begin().await.unwrap();

        match FileZone::get_by_name(&mut txn, &zone.name).await {
            Ok(_) => {
                log::debug!("Zone {} already exists, user sent POST", zone.name);
                return error_result_json!("Zone already exists!", StatusCode::BAD_REQUEST);
            }
            Err(err) => match err {
                sqlx::Error::RowNotFound => {}
                _ => {
                    log::debug!(
                        "Couldn't get zone  {}, something went wrong: {err:?}",
                        zone.name
                    );
                    return error_result_json!(
                        "Server error querying zone!",
                        StatusCode::INTERNAL_SERVER_ERROR
                    );
                }
            },
        };

        // if they got here there were no issues with querying the DB and it doesn't exist already!

        if let Err(err) = zone.save_with_txn(&mut txn).await {
            log::debug!(
                "Couldn't create zone  {}, something went wrong during save: {err:?}",
                zone.name
            );
            return error_result_json!(
                "Server error creating zone!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }

        if let Err(err) = txn.commit().await {
            log::debug!(
                "Couldn't create zone {}, something went wrong committing transaction: {err:?}",
                zone.name
            );
            return error_result_json!(
                "Server error creating zone!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
        // start a new transaction!
        let mut txn = state.connpool().await.begin().await.unwrap();

        let zone = FileZone::get_by_name(&mut txn, &zone.name).await.unwrap();

        let ownership = ZoneOwnership {
            id: None,
            userid: user.id.unwrap(),
            zoneid: zone.id,
        };

        if let Err(err) = ownership.save_with_txn(&mut txn).await {
            log::debug!(
                "Couldn't store zone ownership {ownership:?}, something went wrong: {err:?}"
            );
            return error_result_json!(
                "Server error creating zone ownership, contact the admins!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        };

        if let Err(err) = txn.commit().await {
            log::debug!(
                "Couldn't create zone {}, something went wrong committing transaction: {err:?}",
                zone.name
            );
            return error_result_json!(
                "Server error creating zone, contact the admins!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
        log::debug!("Zone created by user={} zone={zone:?}", user.id.unwrap());
        return Ok(Json("Zone creation completed!".to_string()));
    }

    async fn api_delete(
        state: Extension<SharedState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)> {
        let id = id;

        // get the user
        let user: User = match session.get("user") {
            Some(val) => val,
            None => {
                log::error!("api_delete for filezone and user not found in session!");
                return error_result_json!("", StatusCode::FORBIDDEN);
            }
        };

        // let zone_id: i64 = match payload.get("id") {
        //     None => {
        //         log::debug!("api_delete for filezone and zone id not specified!");
        //         return error_result_json!(
        //             "Please include the field 'id' to specify the zone's domain id.",
        //             StatusCode::BAD_REQUEST
        //         );
        //     }
        //     Some(val) => val.as_i64().unwrap(),
        // };

        let mut txn = match state.connpool().await.begin().await {
            Ok(val) => val,
            Err(err) => {
                log::error!("Error getting txn: {err:?}");
                return error_result_json!(
                    "Internal server error",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            }
        };
        // get the zoneownership
        let _ = match ZoneOwnership::get_ownership_by_userid(&mut txn, &user.id.unwrap(), &id).await
        {
            Ok(val) => val,
            Err(err) => {
                log::error!(
                    "Failed to get ZoneOwnership userid={} zoneid={} error=\"{err:?}\"",
                    user.id.unwrap(),
                    id
                );
                match err {
                    sqlx::Error::RowNotFound => {
                        return error_result_json!(
                            format!("Zone ID {} not found", id).as_str(),
                            StatusCode::NOT_FOUND
                        )
                    }
                    _ => {
                        return error_result_json!(
                            "Internal server error",
                            StatusCode::INTERNAL_SERVER_ERROR
                        )
                    }
                };
            }
        };

        // get the zone
        let zone = match FileZone::get_with_txn(&mut txn, &id).await {
            Ok(val) => val,
            Err(err) => {
                log::error!(
                    "Failed to get Zone during api_delete zoneid={} error=\"{err:?}\"",
                    id
                );
                return error_result_json!(
                    "Internal server error",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            }
        };

        let res = match zone.delete_with_txn(&mut txn).await {
            Ok(_) => Ok(StatusCode::OK),
            Err(err) => {
                log::error!(
                    "Failed to delete Zone during api_delete zoneid={} error=\"{err:?}\"",
                    id
                );
                return error_result_json!(
                    "Internal server error",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            }
        };

        if let Err(err) = txn.commit().await {
            log::error!("Failed to commit txn for zone.delete during api_delete zoneid={} error=\"{err:?}\"", id);
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        };
        res
    }
    async fn api_get(
        state: Extension<SharedState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)> {
        // get the user
        let _user: User = match session.get("user") {
            Some(val) => val,
            None => {
                log::error!("api_delete for filezone and user not found in session!");
                return error_result_json!("", StatusCode::FORBIDDEN);
            }
        };

        // TODO: only return zones for users that are allowed them
        log::debug!("Searching for zone id {id:?}");
        let zone = match FileZone::get(&state.connpool().await, id).await {
            Ok(val) => val,
            Err(err) => todo!("{err:?}"),
        };

        Ok(Json::from(zone))
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

pub fn new() -> Router {
    Router::new()
        // just zone things
        .route("/zone", post(FileZone::api_create))
        .route("/zone/:id", get(FileZone::api_get))
        .route("/zone/:id", delete(FileZone::api_delete))
        // .route("/zone", post(zone_post))
        // .route("/zone/:id", patch(zone_patch))
        // // zone ownership
        // .route("/ownership/:id", get(ownership_get))
        // .route("/ownership/:id", delete(ownership_delete))
        // .route("/ownership/", post(ownership_post))
        // // record related
        // .route("/record/:id", get(record_get))
        // .route("/record/:id", delete(record_delete))
        // .route("/record", post(record_post))
        // .route("/record/:id", patch(record_patch))
        // // user things
        // .route("/user/:id", get(user_get))
        // .route("/user/:id", delete(user_delete))
        // .route("/user/", post(user_post))
        // .route("/user/:id", patch(user_patch))
        .layer(from_fn(auth::check_auth))
        .route("/login", post(auth::login))
}

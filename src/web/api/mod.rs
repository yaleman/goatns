use super::*;
// use crate::db::DBEntity;

use crate::zones::FileZone;
use axum::extract::Extension;
use axum::routing::post;
use axum::Json;
use serde::Deserialize;
use serde::Serialize;

pub mod auth;

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

#[async_trait]
trait APIEntity {
    async fn api_save(
        state: Extension<SharedState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, Json<ErrorResult>>;
    // async fn api_get(pool: &Pool<Sqlite>, id: i64) -> Result<Json<String>, Json<ErrorResult>>;
    async fn api_delete(
        state: Extension<SharedState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, Json<ErrorResult>>;
}

#[async_trait]
impl APIEntity for FileZone {
    async fn api_save(
        _state: Extension<SharedState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, Json<ErrorResult>> {
        log::debug!("Got payload: {payload:?}");
        log::debug!("Hello? {:?}", payload.get("hello"));

        let zone: FileZone = match serde_json::from_value(payload) {
            Ok(val) => val,
            Err(err) => {
                log::debug!("Failed to deser payload: {err:?}");
                return Err(Json(ErrorResult {
                    message: format!("Invalid payload: {err:?}"),
                }));
            }
        };
        log::debug!("Zone: {zone:?}");
        return Err(Json(ErrorResult {
            message: "Invalid payload".to_string(),
        }));
    }
    // async fn api_get(&self, pool: &Pool<Sqlite>) -> Result<Json<String>, Json<ErrorResult>>{
    // todo!()
    // }
    async fn api_delete(
        _state: Extension<SharedState>,
        Json(_payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, Json<ErrorResult>> {
        todo!()
    }
}

// pub async fn zone_delete() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// // pub async fn zone_get() -> Result<Json<FileZone>,String> {
//     // let res = // FileZone
//     // Json::from(NotImplemented::default())
// // }
// pub async fn zone_patch() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn zone_post() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }

// pub async fn record_delete() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn record_get() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn record_patch() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn record_post() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }

// pub async fn ownership_delete() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn ownership_get(
//     Path(userid): Path<String>,
//     Path(zoneid): Path<String>,
// ) -> Result<Json<ZoneOwnership>, String> {
//     let userid: i64 = userid.parse().unwrap_or(-1);
//     let zoneid: i64 = zoneid.parse().unwrap_or(-1);

//     // TODO ownership_get needs a custom getter in the DB
//     if userid == -1 || zoneid == -1 {
//         return Err(r#"{"message": "invalid userid or zoneid specified"}"#.to_string());
//     }

//     log::debug!("ownership_get userid={userid} zoneid={zoneid}");
//     todo!();
// }
// pub async fn ownership_get_user(
//     Path(userid): Path<String>,
//     Extension(state): Extension<SharedState>,
// ) -> Result<Json<Vec<Arc<ZoneOwnership>>>, String> {
//     log::debug!("starting ownership_get_user");
//     let userid: i64 = userid.parse().unwrap();
//     log::debug!("got userid: {userid:?}");
//     let (tx_oneshot, rx_oneshot) = oneshot::channel();
//     let cmd = Command::GetOwnership {
//         zoneid: None,
//         userid: Some(userid),
//         resp: tx_oneshot,
//     };
//     let state_writer = state.write().await;
//     if let Err(err) = state_writer.tx.send(cmd).await {
//         log::error!("Failed to send GetOwnership for userid: {userid} {err:?}");
//         let res = format!("Failed to send GetOwnership for userid: {userid} {err:?}");
//         return Err(res);
//     };

//     let ds_response = match rx_oneshot.await {
//         Ok(val) => val,
//         Err(err) => {
//             log::error!("Failed to GetOwnership for userid: {userid} {err:?}");
//             return Err(r#"{"message": "invalid userid or zoneid specified"}"#.to_string());
//         }
//     };
//     Ok(Json(ds_response))
// }

// pub async fn ownership_get_zone() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }

// pub async fn ownership_post(Json(payload): Json<ZoneOwnership>) -> Result<Json<ZoneOwnership>, ()> {
//     todo!("{payload:?}");
//     // let req_json: String = match req.body_json().await {
//     //     Ok(json) => json,
//     //     Err(err) => {
//     //         log::error!("Failed to deserialize body: {err:?}");
//     //         return Err(tide::Error::from_str(
//     //             tide::StatusCode::InternalServerError,
//     //             "Failed to send request to backend".to_string(),
//     //         ));
//     //     }
//     // };
//     // eprintln!("got body: {req_json:?}");

//     // let ownership: ZoneOwnership = match serde_json::from_str(&req_json) {
//     //     Ok(zo) => zo,
//     //     Err(_) => todo!(),
//     // };
//     // log::debug!("Deser: {ownership:?}");

//     // let response = serde_json::to_string(&NotImplemented::default()).unwrap();
//     // tide_result_json!(response, 403)
// }

// pub async fn user_delete() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn user_get() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn user_patch() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
// }
// pub async fn user_post() -> Json<NotImplemented> {
//     Json::from(NotImplemented::default())
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

pub fn new() -> Router {
    Router::new()
        // just zone things
        // .route("/zone/:id", get(zone_get))
        .route("/zone", post(FileZone::api_save))
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

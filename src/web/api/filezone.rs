use super::*;
use crate::db::DBEntity;

use crate::db::User;
use crate::db::ZoneOwnership;
use crate::error_result_json;
use crate::utils::check_valid_tld;
use crate::zones::FileZone;
use axum::extract::Path;
use axum::Json;
use axum_sessions::extractors::ReadableSession;
use goatns_macros::check_api_auth;
use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileZoneResponse {
    pub message: String,
    pub zone: Option<FileZone>,
    pub id: Option<i64>,
}

#[async_trait]
impl APIEntity for FileZone {
    async fn api_create(
        State(state): State<GoatState>,
        session: ReadableSession,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)> {
        #[cfg(test)]
        println!("Got api_create payload: {payload:?}");
        log::debug!("Got api_create payload: {payload:?}");

        check_api_auth!();

        let zone: FileZone = match serde_json::from_value(payload) {
            Ok(val) => val,
            Err(err) => {
                log::debug!("Failed to deser payload: {err:?}");
                let res = format!("Invalid payload: {err:?}");
                return error_result_json!(res.as_str(), StatusCode::BAD_REQUEST);
            }
        };

        if !check_valid_tld(&zone.name, &state.read().await.config.allowed_tlds) {
            return error_result_json!("Invalid TLD for this system", StatusCode::BAD_REQUEST);
        }

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
            zoneid: zone.id.unwrap(),
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

        return Ok(Json(zone));
    }

    async fn api_update(
        State(state): State<GoatState>,
        session: ReadableSession,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)> {
        check_api_auth!();

        println!("{user:?}");
        #[cfg(test)]
        println!("zone api_update payload: {payload:?}");
        #[cfg(not(test))]
        log::debug!("zone api_update payload: {payload:?}");

        // deserialize the zone
        println!("about to deser input");
        let zone: FileZone = match serde_json::from_value(payload) {
            Ok(val) => val,
            Err(err) => {
                // TODO: make this a better log
                println!("Failed to deserialize user input in api_update: {err:?}");
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResult {
                        message: "Invalid input".to_string(),
                    }),
                ));
            }
        };
        println!("deser'd zone: {zone:?}");

        if zone.id.is_none() {
            return error_result_json!("No zone ID specified", StatusCode::BAD_REQUEST);
        }
        if !check_valid_tld(&zone.name, &state.read().await.config.allowed_tlds) {
            return error_result_json!("Invalid TLD for this system", StatusCode::BAD_REQUEST);
        }

        // get a db transaction
        let connpool = state.connpool().await.clone();
        // TODO getting a transaction might fail
        let mut txn = match connpool.begin().await {
            Ok(val) => val,
            Err(err) => {
                log::error!("failed to get connection to the database: {err:?}");
                return error_result_json!(
                    "Failed to get a connection to the database!",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            }
        };
        // check the user owns the zone
        if let Err(err) =
            ZoneOwnership::get_ownership_by_userid(&mut txn, &user.id.unwrap(), &zone.id.unwrap())
                .await
        {
            // TODO: make this a better log
            println!("Failed to validate user owns zone: {err:?}");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResult {
                    message: "".to_string(),
                }),
            ));
        };
        println!("looks like user owns zone");

        // save the zone data

        if let Err(err) = zone.save_with_txn(&mut txn).await {
            // TODO: make this a better log
            println!("Failed to save zone: {err:?}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResult {
                    message: "failed to save zone".to_string(),
                }),
            ));
        };
        if let Err(err) = txn.commit().await {
            // TODO: make this a better log
            println!("Failed to commit transaction while saving zone: {err:?}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResult {
                    message: "failed to save zone".to_string(),
                }),
            ));
        };
        Ok(Json("success".to_string()))
    }

    async fn api_delete(
        State(state): State<GoatState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)> {
        // let id = id;

        check_api_auth!();

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
        match ZoneOwnership::get_ownership_by_userid(&mut txn, &user.id.unwrap(), &id).await {
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
        State(state): State<GoatState>,
        session: ReadableSession,
        Path(id): Path<i64>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)> {
        check_api_auth!();

        let mut txn = state.connpool().await.begin().await.unwrap();

        if ZoneOwnership::get_ownership_by_userid(&mut txn, &user.id.unwrap(), &id)
            .await
            .is_err()
        {
            log::error!(
                "User {} not authorized for zoneid={}",
                &user.id.unwrap(),
                id
            );
            return error_result_json!("", StatusCode::UNAUTHORIZED);
        };

        log::debug!("Searching for zone id {id:?}");
        let zone = match FileZone::get(&state.connpool().await, id).await {
            Ok(val) => val,
            Err(err) => todo!("{err:?}"),
        };

        Ok(Json::from(zone))
    }
}

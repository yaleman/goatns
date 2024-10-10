use super::*;
use crate::db::DBEntity;

use crate::db::User;
use crate::db::ZoneOwnership;
use crate::error_result_json;
use crate::utils::check_valid_tld;
use crate::zones::FileZone;
use axum::extract::Path;
use axum::Json;
use goatns_macros::check_api_auth;
use serde::Deserialize;
use serde::Serialize;
use tower_sessions::Session;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileZoneResponse {
    pub message: String,
    pub zone: Option<FileZone>,
    pub id: Option<i64>,
}

pub(crate) async fn api_create(
    State(state): State<GoatState>,
    session: Session,
    Json(zone): Json<FileZone>,
) -> Result<Json<Box<FileZone>>, (StatusCode, Json<ErrorResult>)> {
    check_api_auth!();

    if !check_valid_tld(&zone.name, &state.read().await.config.allowed_tlds) {
        return error_result_json!("Invalid TLD for this system", StatusCode::BAD_REQUEST);
    }

    // check to see if the zone exists
    let mut txn = match state.connpool().await.begin().await {
        Ok(val) => val,
        Err(err) => {
            log::error!("failed to get connection to the database: {err:?}");
            return error_result_json!(
                "Failed to get a connection to the database!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    match FileZone::get_by_name(&mut txn, &zone.name).await {
        Ok(Some(_)) => {
            log::debug!("Zone {} already exists, user sent POST", zone.name);
            return error_result_json!("Zone already exists!", StatusCode::BAD_REQUEST);
        }
        Ok(None) => {}
        Err(err) => {
            log::debug!(
                "Couldn't get zone  {}, something went wrong: {err:?}",
                zone.name
            );
            return error_result_json!(
                "Server error querying zone!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
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
    let mut txn = match state.connpool().await.begin().await {
        Ok(val) => val,
        Err(err) => {
            log::error!("failed to get connection to the database: {err:?}");
            return error_result_json!(
                "Failed to get a connection to the database!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let zone = match FileZone::get_by_name(&mut txn, &zone.name).await {
        Ok(val) => match val {
            Some(val) => *val,
            None => {
                return error_result_json!("Couldn't find Zone!", StatusCode::NOT_FOUND);
            }
        },
        Err(err) => {
            log::debug!(
                "Couldn't get zone  {}, something went wrong: {err:?}",
                zone.name
            );
            return error_result_json!(
                "Server error querying zone!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let userid = match user.id {
        Some(val) => val,
        None => {
            log::debug!("User id not found in session, something went wrong");
            return error_result_json!(
                "Server error creating zone, contact the admins!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let zoneid = match zone.id {
        Some(val) => val,
        None => {
            log::debug!("Zone id not found in session, something went wrong");
            return error_result_json!(
                "Server error creating zone, contact the admins!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let ownership = ZoneOwnership {
        id: None,
        userid,
        zoneid,
    };

    if let Err(err) = ownership.save_with_txn(&mut txn).await {
        log::debug!("Couldn't store zone ownership {ownership:?}, something went wrong: {err:?}");
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
    log::debug!("Zone created by user={:?} zone={:?}", user.id, zone);

    Ok(Json(Box::new(zone)))
}

pub(crate) async fn api_update(
    State(state): State<GoatState>,
    session: Session,
    Json(zone): Json<FileZone>,
) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)> {
    check_api_auth!();

    let zone_id = match zone.id {
        Some(val) => val,
        None => {
            return error_result_json!("No zone ID specified", StatusCode::BAD_REQUEST);
        }
    };
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

    let user_id = match user.id {
        Some(val) => val,
        None => {
            log::error!("User id not found in session, something went wrong");
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // check the user owns the zone
    if let Err(err) = ZoneOwnership::get_ownership_by_userid(&mut txn, &user_id, &zone_id).await {
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

pub(crate) async fn api_delete(
    State(state): State<GoatState>,
    session: Session,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)> {
    // let id = id;

    check_api_auth!();

    let mut txn = match state.connpool().await.begin().await {
        Ok(val) => val,
        Err(err) => {
            log::error!("Error getting txn: {err:?}");
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    // get the zoneownership
    let userid = match user.id {
        Some(val) => val,
        None => {
            log::error!("User id not found in session, something went wrong");
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    match ZoneOwnership::get_ownership_by_userid(&mut txn, &userid, &id).await {
        Ok(None) => {
            return error_result_json!(
                format!("Zone ID {} not found", id).as_str(),
                StatusCode::NOT_FOUND
            )
        }
        Ok(Some(val)) => val,
        Err(err) => {
            error!(
                "Failed to get zone ownership for zoneid={}: error: {:?}",
                id, err
            );
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
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
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let res = match zone.delete_with_txn(&mut txn).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(err) => {
            log::error!(
                "Failed to delete Zone during api_delete zoneid={} error=\"{err:?}\"",
                id
            );
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Err(err) = txn.commit().await {
        log::error!(
            "Failed to commit txn for zone.delete during api_delete zoneid={} error=\"{err:?}\"",
            id
        );
        return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
    };
    res
}

pub(crate) async fn api_get(
    State(state): State<GoatState>,
    session: Session,
    Path(id): Path<i64>,
) -> Result<Json<Box<FileZone>>, (StatusCode, Json<ErrorResult>)> {
    check_api_auth!();

    let mut txn = match state.connpool().await.begin().await {
        Ok(val) => val,
        Err(err) => {
            log::error!("failed to get connection to the database: {err:?}");
            return error_result_json!(
                "Failed to get a connection to the database!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let user_id = match user.id {
        Some(val) => val,
        None => {
            log::error!("User id not found in session, something went wrong");
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if ZoneOwnership::get_ownership_by_userid(&mut txn, &user_id, &id)
        .await
        .is_err()
    {
        log::error!("User {:?} not authorized for zoneid={}", &user_id, id);
        return error_result_json!("", StatusCode::UNAUTHORIZED);
    };

    log::debug!("Searching for zone id {id:?}");
    let zone = match FileZone::get(&state.connpool().await, id).await {
        Ok(val) => val,
        Err(err) => {
            error!("Couldn't get zone id {}: error: {:?}", id, err);
            return error_result_json!(
                format!("Couldn't get zone id {}", id).as_ref(),
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    Ok(Json::from(zone))
}

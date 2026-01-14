use super::*;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue::Set;
use sea_orm::ModelTrait;

use sea_orm::TransactionTrait;

use crate::db::entities;
use crate::error_result_json;
use crate::utils::check_valid_tld;
use crate::web::api::filezonerecord::ZoneForm;

use axum::Json;
use axum::extract::Path;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use serde::Deserialize;
use serde::Serialize;
use tower_sessions::Session;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileZoneResponse {
    pub message: String,
    pub zone: Option<ZoneForm>,
    pub id: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ApiZoneResponse {
    #[serde(flatten)]
    pub zone: entities::zones::Model,
    pub records: Vec<entities::records::Model>,
}

impl From<entities::zones::Model> for ApiZoneResponse {
    fn from(value: entities::zones::Model) -> Self {
        // TODO: get the records for the zone
        ApiZoneResponse {
            zone: value,
            records: vec![],
        }
    }
}

#[utoipa::path(
    method(post),
    path = "/api/zone",
    responses(
        (status = OK, description = "Success", body = str, content_type = "text/plain")
    )
)]
#[axum::debug_handler]
pub(crate) async fn api_create(
    State(state): State<GoatState>,
    session: Session,
    Json(zone): Json<ZoneForm>,
) -> Result<Json<ApiZoneResponse>, (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

    if !check_valid_tld(&zone.name, &state.read().await.config.allowed_tlds) {
        return error_result_json!("Invalid TLD for this system", StatusCode::BAD_REQUEST);
    }

    // check to see if the zone exists
    let mut txn = state.get_db_txn().await.map_err(|err| {
        error!("failed to get connection to the database: {err:?}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResult {
                message: "Failed to get a connection to the database!".to_string(),
            }),
        )
    })?;

    match entities::zones::Entity::find()
        .filter(entities::zones::Column::Name.eq(&zone.name))
        .one(&mut txn)
        .await
    {
        Ok(Some(_)) => {
            debug!("Zone {} already exists, user sent POST", zone.name);
            return error_result_json!("Zone already exists!", StatusCode::BAD_REQUEST);
        }
        Ok(None) => {}
        Err(err) => {
            debug!(
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

    let zone: entities::zones::ActiveModel = zone.into();

    let zone = zone.clone().insert(&mut txn).await.map_err(|err| {
        debug!(
            "Couldn't create zone  {}, something went wrong during save: {err:?}",
            zone.name.as_ref()
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResult {
                message: "Server error creating zone!".to_string(),
            }),
        )
    })?;

    let userid = match user.id {
        Some(val) => val,
        None => {
            debug!("User id not found in session, something went wrong");
            return error_result_json!(
                "Server error creating zone, contact the admins!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let ownership = entities::ownership::ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        userid: Set(userid),
        zoneid: Set(zone.id),
    };

    if let Err(err) = ownership.clone().insert(&mut txn).await {
        debug!("Couldn't store zone ownership {ownership:?}, something went wrong: {err:?}");
        return error_result_json!(
            "Server error creating zone ownership, contact the admins!",
            StatusCode::INTERNAL_SERVER_ERROR
        );
    };

    if let Err(err) = txn.commit().await {
        debug!(
            "Couldn't create zone {}, something went wrong committing transaction: {err:?}",
            zone.name
        );
        return error_result_json!(
            "Server error creating zone, contact the admins!",
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    debug!("Zone created by user={:?} zone={:?}", user.id, zone);

    Ok(Json(zone.into()))
}

pub(crate) async fn api_zone_update(
    State(state): State<GoatState>,
    session: Session,
    Json(zone): Json<ApiZoneResponse>,
) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

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
            error!("failed to get connection to the database: {err:?}");
            return error_result_json!(
                "Failed to get a connection to the database!",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    let user_id = match user.id {
        Some(val) => val,
        None => {
            error!("User id not found in session, something went wrong");
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // check the user owns the zone
    if let Err(err) = entities::ownership::Entity::find()
        .filter(
            entities::ownership::Column::Userid
                .eq(user_id)
                .and(entities::ownership::Column::Zoneid.eq(zone_id)),
        )
        .one(&mut txn)
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

pub(crate) async fn api_zone_delete(
    State(state): State<GoatState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)> {
    // let id = id;

    let user = check_api_auth(&session).await?;

    let mut txn = state.get_db_txn().await.map_err(|err| {
        error!("Error getting txn: {err:?}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Internal server error")),
        )
    })?;
    // get the zoneownership
    let userid = match user.id {
        Some(val) => val,
        None => {
            error!("User id not found in session, something went wrong");
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let (ownership, Some(zone)) = match entities::ownership::Entity::find_by_id(id)
        .filter(entities::ownership::Column::Userid.eq(userid))
        .find_also_related(entities::zones::Entity)
        .one(&mut txn)
        .await
    {
        Ok(None) => {
            return error_result_json!(
                format!("Zone ID {id} not found").as_str(),
                StatusCode::NOT_FOUND
            );
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

    zone.delete(&mut txn).await.map_err(|err| {
        error!(
            "Failed to delete Zone during api_delete zoneid={} error=\"{err:?}\"",
            id
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Internal server error")),
        )
    })?;

    if let Err(err) = txn.commit().await {
        error!(
            "Failed to commit txn for zone.delete during api_delete zoneid={} error=\"{err:?}\"",
            id
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Internal server error")),
        ));
    };
    Ok(StatusCode::OK)
}

pub(crate) async fn api_get(
    State(state): State<GoatState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiZoneResponse>, (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

    let mut txn = state.get_db_txn().await.map_err(|err| {
        error!("Error getting txn: {err:?}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Internal server error")),
        )
    })?;

    let data = entities::ownership::Entity::find()
        .filter(
            entities::ownership::Column::Userid
                .eq(user.id)
                .and(entities::ownership::Column::Zoneid.eq(id)),
        )
        .find_with_linked(entities::zones::Entity)
        .all(&mut txn)
        .await
        .map_err(|err| {
            error!("Error checking ownership: {err:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(ErrorResult::from("Internal server error")),
            )
        })?;

    debug!("Searching for zone id {id:?}");
    let zone = match FileZone::get(&state.connpool().await, id).await {
        Ok(val) => val,
        Err(err) => {
            error!("Couldn't get zone id {}: error: {:?}", id, err);
            return error_result_json!(
                format!("Couldn't get zone id {id}").as_ref(),
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    };

    Ok(Json::from(zone))
}

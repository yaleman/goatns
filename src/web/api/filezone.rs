use super::*;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue::Set;
use sea_orm::IntoActiveModel;
use sea_orm::ModelTrait;

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
    let txn = state.get_db_txn().await.map_err(|err| {
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
        .one(&txn)
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

    let zone = zone.clone().insert(&txn).await.map_err(|err| {
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

    if let Err(err) = ownership.clone().insert(&txn).await {
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct ZoneUpdate {
    pub id: Uuid,
    pub rname: Option<String>,
    pub serial: Option<u32>,
    pub refresh: Option<u32>,
    pub retry: Option<u32>,
    pub expire: Option<u32>,
    pub minimum: Option<u32>,
}

pub(crate) async fn api_zone_update(
    State(state): State<GoatState>,
    session: Session,
    Json(zone_form): Json<ZoneUpdate>,
) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

    let txn = state.get_db_txn().await.map_err(|err| {
        error!("failed to get connection to the database: {err:?}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from(
                "Failed to get a connection to the database!",
            )),
        )
    })?;

    let Some(user_id) = user.id else {
        error!("User id not found in session, something went wrong");
        return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
    };

    // check the user owns the zone
    let Some((_ownership, Some(zone))) = entities::ownership::Entity::find()
        .filter(
            entities::ownership::Column::Userid
                .eq(user_id)
                .and(entities::ownership::Column::Zoneid.eq(zone_form.id)),
        )
        .find_also_related(entities::zones::Entity)
        .one(&txn)
        .await
        .map_err(|err| {
            // TODO: make this a better log
            println!("Failed to validate user owns zone: {err:?}");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResult {
                    message: "".to_string(),
                }),
            )
        })?
    else {
        error!("User {} does not own zone {}", user_id, zone_form.id);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResult {
                message: "You do not own this zone".to_string(),
            }),
        ));
    };
    println!("looks like user owns zone");

    // save the zone data

    let mut zone = zone.into_active_model();

    if let Some(rname) = zone_form.rname {
        zone.rname.set_if_not_equals(rname);
    }
    if let Some(serial) = zone_form.serial {
        zone.serial.set_if_not_equals(serial);
    }
    if let Some(refresh) = zone_form.refresh {
        zone.refresh.set_if_not_equals(refresh);
    }
    if let Some(retry) = zone_form.retry {
        zone.retry.set_if_not_equals(retry);
    }
    if let Some(expire) = zone_form.expire {
        zone.expire.set_if_not_equals(expire);
    }
    if let Some(minimum) = zone_form.minimum {
        zone.minimum.set_if_not_equals(minimum);
    }

    if zone.is_changed() {
        println!("Zone has changes, updating...");
        if let Err(err) = zone.save(&txn).await {
            // TODO: make this a better log
            error!("Failed to save zone: {err:?}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResult {
                    message: "failed to save zone".to_string(),
                }),
            ));
        };
        if let Err(err) = txn.commit().await {
            // TODO: make this a better log
            error!("Failed to commit transaction while saving zone: {err:?}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResult {
                    message: "failed to save zone".to_string(),
                }),
            ))
        } else {
            Ok(Json("success".to_string()))
        }
    } else {
        debug!("Zone has no changes, skipping update...");
        Ok(Json("success".to_string()))
    }
}

pub(crate) async fn api_zone_delete(
    State(state): State<GoatState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)> {
    // let id = id;

    let user = check_api_auth(&session).await?;

    let txn = state.get_db_txn().await.map_err(|err| {
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

    let zone = match entities::ownership::Entity::find_by_id(id)
        .filter(entities::ownership::Column::Userid.eq(userid))
        .find_also_related(entities::zones::Entity)
        .one(&txn)
        .await
    {
        Ok(Some((_, None))) | Ok(None) => {
            return error_result_json!(
                format!("Zone ID {id} not found").as_str(),
                StatusCode::NOT_FOUND
            );
        }
        Ok(Some((_ownership, Some(zone)))) => zone,
        Err(err) => {
            error!(
                "Failed to get zone ownership for zoneid={}: error: {:?}",
                id, err
            );
            return error_result_json!("Internal server error", StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    zone.delete(&txn).await.map_err(|err| {
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

    let txn = state.get_db_txn().await.map_err(|err| {
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
        .find_also_related(entities::zones::Entity)
        .one(&txn)
        .await
        .map_err(|err| {
            error!("Error checking ownership: {err:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(ErrorResult::from("Internal server error")),
            )
        })?;

    let zone = match data {
        Some((_, Some(zone))) => zone,
        _ => {
            error!("Zone ID {} not found for user {:?}", id, user.id);
            return error_result_json!("Zone not found", StatusCode::NOT_FOUND);
        }
    };

    let records = zone
        .find_related(entities::records::Entity)
        .all(&txn)
        .await
        .map_err(|err| {
            error!("Error getting records for zone {}: {err:?}", zone.name);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(ErrorResult::from("Internal server error")),
            )
        })?;

    Ok(Json::from(ApiZoneResponse {
        zone: zone.clone(),
        records,
    }))
}

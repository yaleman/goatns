use crate::db::entities;
use crate::enums::{RecordClass, RecordType};
use crate::error_result_json;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter};
use tower_sessions::Session;
use tracing::debug;
use utoipa::ToSchema;
use uuid::Uuid;

use super::*;

#[derive(Deserialize, Serialize, Debug, ToSchema, Clone)]
pub struct ZoneForm {
    pub id: Option<Uuid>,
    pub name: String,
    pub rname: String,
    pub serial: u32,
    pub refresh: u32,
    pub retry: u32,
    pub expire: u32,
    pub minimum: u32,
}

#[derive(Deserialize, Serialize, Debug, ToSchema, Clone)]
pub struct ZoneRecordForm {
    pub id: Option<Uuid>,
    pub name: String,
    pub rclass: RecordClass,
    pub rrtype: RecordType,
    pub rdata: String,
    pub ttl: Option<u32>,
    pub zoneid: Uuid,
}

impl From<entities::zones::Model> for ZoneForm {
    fn from(zone: entities::zones::Model) -> Self {
        ZoneForm {
            id: Some(zone.id),
            name: zone.name,
            rname: zone.rname,
            serial: zone.serial,
            refresh: zone.refresh,
            retry: zone.retry,
            expire: zone.expire,
            minimum: zone.minimum,
        }
    }
}

/// Save the entity to the database
#[utoipa::path(
    post,
    path = "/api/record",
    operation_id = "record_create",
    request_body = ZoneForm,
    responses(
        (status = 200, description = "Successful"),
        (status = 403, description = "Auth failed"),
        (status = 500, description = "Something broke!"),
    ),
    tag = "Records",
)]

pub(crate) async fn api_create(
    State(state): State<GoatState>,
    session: Session,
    Json(record): Json<ZoneForm>,
) -> Result<Json<entities::zones::Model>, (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

    let user_id = match user.id {
        Some(val) => val,
        None => {
            debug!("No user id found in session");
            return error_result_json!("No user id found in session", StatusCode::UNAUTHORIZED);
        }
    };

    let txn = state.get_db_txn().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Database error")),
        )
    })?;
    debug!(
        "looking for ZO for user: {} zoneid: {:?}",
        user_id, record.id
    );
    let zone = entities::zones::ActiveModel::from(record);

    match zone.insert(&txn).await.map_err(GoatNsError::from) {
        Err(err) => {
            eprintln!("Error saving record: {err:?}");

            // Check if this is a duplicate record error or validation error
            match &err {
                crate::error::GoatNsError::Generic(msg) => {
                    if msg.contains("Record with same zone, name, type, and class already exists") {
                        return error_result_json!(
                            "A record with the same name, type, and class already exists in this zone",
                            StatusCode::CONFLICT
                        );
                    }
                    if msg.contains("Record must have a valid zone ID") {
                        return error_result_json!(
                            "Record must have a valid zone ID",
                            StatusCode::BAD_REQUEST
                        );
                    }
                    if msg.contains("Record name cannot be empty") {
                        return error_result_json!(
                            "Record name cannot be empty",
                            StatusCode::BAD_REQUEST
                        );
                    }
                }
                crate::error::GoatNsError::SqlxError(sqlx::Error::Database(db_err)) => {
                    if let Some(constraint) = db_err.constraint() {
                        if constraint == "ind_records" {
                            return error_result_json!(
                                "A record with the same name, type, and class already exists in this zone",
                                StatusCode::CONFLICT
                            );
                        }
                    }
                    // Check for unique constraint error codes
                    if db_err.code() == Some(std::borrow::Cow::Borrowed("2067"))
                        || db_err.code() == Some(std::borrow::Cow::Borrowed("1555"))
                    {
                        return error_result_json!(
                            "A record with the same name, type, and class already exists in this zone",
                            StatusCode::CONFLICT
                        );
                    }
                }
                _ => {}
            }

            error_result_json!("Error saving record", StatusCode::BAD_REQUEST)
        }
        Ok(val) => {
            if let Err(err) = txn.commit().await {
                // TODO: This error message needs improving
                eprintln!("error committing transaction! {err:?}");
                return error_result_json!(
                    "Error saving record, see the admins",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            }
            Ok(Json(val))
        }
    }
}

#[derive(Deserialize, Serialize, Debug, ToSchema, Clone)]
pub struct ApiRecordUpdate {
    pub id: Uuid,
    pub name: Option<String>,
    pub ttl: Option<u32>,
    pub rrtype: Option<u16>,
    pub rclass: Option<u16>,
    pub rdata: Option<String>,
}

/// HTTP Put <https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/PUT>
pub(crate) async fn api_update(
    State(state): State<GoatState>,
    session: Session,
    Json(payload): Json<ApiRecordUpdate>,
) -> Result<Json<entities::records::Model>, (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

    let txn = state.get_db_txn().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Database error")),
        )
    })?;

    let user_id = match user.id {
        Some(val) => val,
        None => {
            debug!("No user id found in session");
            return error_result_json!("No user id found in session", StatusCode::UNAUTHORIZED);
        }
    };

    let record = entities::records::Entity::find_by_id(payload.id)
        .find_also_related(entities::zones::Entity)
        .one(&txn)
        .await
        .map_err(|err| {
            error!("Error fetching record id {:?}: {err:?}", payload.id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(ErrorResult::from("Database error")),
            )
        })?;

    let (record, zone) = match record {
        Some((record, Some(zone))) => (record, zone),
        _ => {
            debug!(
                "Record id {:?} not found or zone missing for user {:?}",
                payload.id, user_id
            );
            return error_result_json!("Record not found", StatusCode::NOT_FOUND);
        }
    };

    // check ownership
    let Some(_ownership) = entities::ownership::Entity::find()
        .filter(
            entities::ownership::Column::Zoneid
                .eq(zone.id)
                .and(entities::ownership::Column::Userid.eq(user_id)),
        )
        .one(&txn)
        .await
        .map_err(|err| {
            error!("Error checking ownership: {err:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(ErrorResult::from("Database error")),
            )
        })?
    else {
        debug!("User {:?} does not own zone id {:?}", user_id, zone.id);
        return error_result_json!(
            "Not authorized to modify this record",
            StatusCode::FORBIDDEN
        );
    };

    let mut record_am: entities::records::ActiveModel = record.into();
    if let Some(name) = payload.name {
        record_am.name = sea_orm::Set(name);
    }
    if let Some(ttl) = payload.ttl {
        record_am.ttl = sea_orm::Set(Some(ttl));
    }
    if let Some(rrtype) = payload.rrtype {
        record_am.rrtype = sea_orm::Set(rrtype);
    }
    if let Some(rclass) = payload.rclass {
        record_am.rclass = sea_orm::Set(rclass);
    }
    if let Some(rdata) = payload.rdata {
        record_am.rdata = sea_orm::Set(rdata);
    }
    let res = record_am.update(&txn).await.map_err(|err| {
        error!("Error updating record id {:?}: {err:?}", payload.id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Database error")),
        )
    })?;

    Ok(Json(res))
}
pub(crate) async fn api_get(
    State(state): State<GoatState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<Json<entities::records::Model>, (StatusCode, Json<ErrorResult>)> {
    check_api_auth(&session).await?;
    let txn = state.get_db_txn().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Database error")),
        )
    })?;

    match entities::records::Entity::find_by_id(id).one(&txn).await {
        Ok(Some(val)) => Ok(Json(val)),
        Ok(None) => {
            error_result_json!("Record not found", StatusCode::NOT_FOUND)
        }
        Err(err) => {
            // TODO: this should handle missing OR failures
            eprintln!("Error getting record: {err:?}");
            error_result_json!("", StatusCode::NOT_FOUND)
        }
    }
}

/// Delete an object
/// <https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/DELETE>
pub(crate) async fn api_delete_zone(
    State(state): State<GoatState>,
    session: Session,
    Path(zone_id): Path<Uuid>,
) -> Result<(), (StatusCode, Json<ErrorResult>)> {
    let user = check_api_auth(&session).await?;

    let txn = state.get_db_txn().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Database error")),
        )
    })?;

    let Some(user_id) = user.id else {
        debug!("No user id found in session");
        return error_result_json!("No user id found in session", StatusCode::UNAUTHORIZED);
    };

    let ownership_zone = entities::ownership::Entity::find()
        .filter(
            entities::ownership::Column::Zoneid
                .eq(zone_id)
                .and(entities::ownership::Column::Userid.eq(user_id)),
        )
        .find_also_related(entities::zones::Entity)
        .one(&txn)
        .await
        .map_err(|err| {
            eprintln!("Error checking ownership: {err:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(ErrorResult::from("Database error")),
            )
        })?;

    let Some((_ownership, Some(zone))) = ownership_zone else {
        error!("User {:?} does not own zone id {:?}", user.id, zone_id);
        return error_result_json!("Not authorized to delete this zone", StatusCode::FORBIDDEN);
    };

    zone.delete(&txn).await.map_err(|err| {
        eprintln!("Error deleting zone id {:?}: {err:?}", zone_id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ErrorResult::from("Database error")),
        )
    })?;

    if let Err(err) = txn.commit().await {
        // TODO: This error message needs improving
        eprintln!("error committing transaction! {err:?}");
        return error_result_json!(
            "Error deleting record, see the admins",
            StatusCode::INTERNAL_SERVER_ERROR
        );
    };

    Ok(())
}

use crate::db::{DBEntity, User, ZoneOwnership};
use crate::error_result_json;
use crate::zones::FileZoneRecord;
use goatns_macros::check_api_auth;
use tower_sessions::Session;

use super::*;

#[async_trait]
impl APIEntity for FileZoneRecord {
    /// Save the entity to the database
    async fn api_create(
        State(state): State<GoatState>,
        session: Session,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)> {
        check_api_auth!();

        let record: Self = match serde_json::from_value(payload) {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Failed to parse object: {err:?}");
                return error_result_json!("Failed to parse object", StatusCode::BAD_REQUEST);
            }
        };

        let mut txn = state.connpool().await.begin().await.unwrap();
        println!(
            "looking for ZO for user: {} zoneid: {}",
            user.id.unwrap(),
            record.zoneid.unwrap()
        );
        if let Err(err) = ZoneOwnership::get_ownership_by_userid(
            &mut txn,
            &user.id.unwrap(),
            &record.zoneid.unwrap(),
        )
        .await
        {
            eprintln!("Error getting ownership: {err:?}");
            return error_result_json!("", StatusCode::UNAUTHORIZED);
        };

        match record.save_with_txn(&mut txn).await {
            Err(err) => {
                eprintln!("Error saving record: {err:?}");
                // TODO: this needs to handle index conflicts
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
    /// HTTP Put <https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/PUT>
    async fn api_update(
        State(state): State<GoatState>,
        session: Session,
        Json(payload): Json<serde_json::Value>,
    ) -> Result<Json<String>, (StatusCode, Json<ErrorResult>)> {
        check_api_auth!();

        let record: Self = match serde_json::from_value(payload) {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Failed to parse object: {err:?}");
                return error_result_json!("Failed to parse object", StatusCode::BAD_REQUEST);
            }
        };
        let mut txn = state.connpool().await.begin().await.unwrap();

        let res = match record.update_with_txn(&mut txn).await {
            Ok(val) => val,
            Err(err) => {
                // TODO: this should handle missing OR failures
                eprintln!("Error getting record: {err:?}");
                return error_result_json!("", StatusCode::NOT_FOUND);
            }
        };

        if let Err(err) = ZoneOwnership::get_ownership_by_userid(
            &mut txn,
            &user.id.unwrap(),
            &res.zoneid.unwrap(),
        )
        .await
        {
            eprintln!("Error getting ownership: {err:?}");
            return error_result_json!("", StatusCode::UNAUTHORIZED);
        };

        Ok(Json(serde_json::to_string(&res).unwrap()))
    }
    async fn api_get(
        State(state): State<GoatState>,
        session: Session,
        Path(id): Path<i64>,
    ) -> Result<Json<Box<Self>>, (StatusCode, Json<ErrorResult>)> {
        check_api_auth!();

        let pool = state.connpool().await;
        let res = match FileZoneRecord::get(&pool, id).await {
            Ok(val) => val,
            Err(err) => {
                // TODO: this should handle missing OR failures
                eprintln!("Error getting record: {err:?}");
                return error_result_json!("", StatusCode::NOT_FOUND);
            }
        };
        Ok(Json(res))
    }

    /// Delete an object
    /// <https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/DELETE>
    async fn api_delete(
        State(state): State<GoatState>,
        session: Session,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorResult>)> {
        check_api_auth!();

        let mut txn = state.connpool().await.begin().await.unwrap();

        let record = match FileZoneRecord::get_with_txn(&mut txn, &id).await {
            Ok(val) => val,
            Err(err) => {
                let resmsg = format!("error getting record: {err:?}");
                return error_result_json!(resmsg.as_str(), StatusCode::UNAUTHORIZED);
            }
        };

        if let Err(err) = ZoneOwnership::get_ownership_by_userid(
            &mut txn,
            &user.id.unwrap(),
            &record.zoneid.unwrap(),
        )
        .await
        {
            eprintln!("Error getting ownership: {err:?}");
            return error_result_json!("no zone ownership found", StatusCode::UNAUTHORIZED);
        };

        if let Err(err) = record.delete_with_txn(&mut txn).await {
            // TODO: This error message needs improving
            eprintln!("error committing transaction! {err:?}");
            return error_result_json!(
                "Error deleting record, see the admins",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
        if let Err(err) = txn.commit().await {
            // TODO: This error message needs improving
            eprintln!("error committing transaction! {err:?}");
            return error_result_json!(
                "Error deleting record, see the admins",
                StatusCode::INTERNAL_SERVER_ERROR
            );
        };

        Ok(StatusCode::OK)
    }
}

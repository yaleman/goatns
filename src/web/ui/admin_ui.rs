use crate::db::entities;
use crate::web::constants::SESSION_USER_KEY;
use crate::web::middleware::admin::require_admin;
use crate::web::utils::Urls;
use crate::web::{GoatState, GoatStateTrait};
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::middleware::from_fn;
use axum::response::Redirect;
use axum::routing::get;
use axum::{Form, Router};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QuerySelect,
};
use serde::Deserialize;
use tower_sessions::Session;
use tracing::{debug, error, instrument};
use uuid::Uuid;

#[derive(Template, WebTemplate)]
#[template(path = "admin_ui.html")]
pub(crate) struct AdminUITemplate /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "admin_report_unowned_records.html")]
pub(crate) struct AdminReportUnownedRecords /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
    pub records: Vec<ZoneRecord>,

    pub zones: Vec<entities::zones::Model>,
}

#[allow(dead_code)] // because this is only used in a template
/// Template struct for showing a zone record
pub(crate) struct ZoneRecord {
    id: u32,
    name: String,
    zoneid: u32,
}

#[instrument(level = "info", skip_all)]
pub(crate) async fn dashboard(session: Session) -> Result<AdminUITemplate, Redirect> {
    let user: entities::users::Model = match session.get(SESSION_USER_KEY).await {
        Ok(Some(user)) => user,
        _ => {
            debug!("Admin dashboard: no valid session user");
            return Err(Redirect::to(&Urls::Login.to_string()));
        }
    };

    Ok(AdminUITemplate {
        user_is_admin: user.admin,
    })
}

pub(crate) async fn report_unowned_records(
    session: Session,
    State(state): State<GoatState>,
) -> Result<AdminReportUnownedRecords, Redirect> {
    let user: entities::users::Model = match session.get(SESSION_USER_KEY).await {
        Ok(Some(user)) => user,
        _ => {
            debug!("Admin report_unowned_records: no valid session user");
            return Err(Redirect::to(&Urls::Login.to_string()));
        }
    };

    let txn = match state.get_db_txn().await {
        Ok(txn) => txn,
        Err(err) => {
            error!("Failed to get DB connection: {err:?}");
            return Err(Redirect::to(&Urls::ZonesList.to_string()));
        }
    };

    // Get all zone IDs that have ownership records
    let owned_zone_ids: Vec<Uuid> = match entities::ownership::Entity::find()
        .select_only()
        .column(entities::ownership::Column::Zoneid)
        .into_tuple()
        .all(&txn)
        .await
    {
        Ok(ids) => ids,
        Err(err) => {
            error!("Failed to query ownership: {err:?}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }
    };

    // Get zones that don't have ownership records
    let unowned_zones = match entities::zones::Entity::find()
        .filter(entities::zones::Column::Id.is_not_in(owned_zone_ids))
        .all(&txn)
        .await
    {
        Ok(zones) => zones,
        Err(err) => {
            error!("Failed to get unowned zones: {err:?}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }
    };

    Ok(AdminReportUnownedRecords {
        user_is_admin: user.admin,
        records: vec![],
        zones: unowned_zones,
    })
}

#[derive(Template, WebTemplate)]
#[template(path = "admin_ownership_template.html")]
pub(crate) struct AssignOwnershipTemplate {
    user_is_admin: bool,
    zone: entities::zones::Model,
    user: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct AssignOwnershipForm {
    // userid: Option<i64>,
    username: Option<String>,
}

#[instrument(level = "info", skip(session, state))]
pub(crate) async fn assign_zone_ownership(
    session: Session,
    State(state): State<GoatState>,
    Path(id): Path<Uuid>,
    Form(form): Form<AssignOwnershipForm>,
) -> Result<AssignOwnershipTemplate, Redirect> {
    let current_user: entities::users::Model = match session.get(SESSION_USER_KEY).await {
        Ok(Some(user)) => user,
        _ => {
            debug!("Admin assign_zone_ownership: no valid session user");
            return Err(Redirect::to(&Urls::Login.to_string()));
        }
    };

    let txn = match state.get_db_txn().await {
        Ok(txn) => txn,
        Err(err) => {
            error!("Failed to get DB transaction: {err:?}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }
    };

    let zone = match entities::zones::Entity::find_by_id(id).one(&txn).await {
        Ok(Some(zone)) => zone,
        Ok(None) => {
            error!("No zone found with ID: {id}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }
        Err(err) => {
            error!("Failed to get zone by ID: {err:?}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }
    };

    if let Some(username) = form.username.as_ref() {
        debug!("Got a username in the form!");
        let target_user = match entities::users::Entity::find()
            .filter(entities::users::Column::Username.eq(username.as_str()))
            .one(&txn)
            .await
        {
            Ok(Some(user)) => user,
            Ok(None) => {
                return Err(Redirect::to(&Urls::Admin.to_string()));
            }
            Err(err) => {
                error!("Failed to get user by name: {err:?}");
                return Err(Redirect::to(&Urls::Admin.to_string()));
            }
        };

        // Create ownership record
        let ownership = entities::ownership::ActiveModel {
            id: sea_orm::ActiveValue::NotSet,
            zoneid: Set(zone.id),
            userid: Set(target_user.id),
        };

        if let Err(err) = ownership.insert(&txn).await {
            error!("Failed to insert zone ownership: {err:?}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }

        if let Err(err) = txn.commit().await {
            error!("Failed to commit transaction: {err:?}");
            return Err(Redirect::to(&Urls::Admin.to_string()));
        }

        return Err(Redirect::to(&Urls::Admin.to_string()));
    }

    Ok(AssignOwnershipTemplate {
        user_is_admin: current_user.admin,
        zone,
        user: form.username,
    })
}

/// Build the router for user settings
pub fn router() -> Router<GoatState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/reports/unowned_records", get(report_unowned_records))
        .route(
            "/zones/assign_ownership/{id}",
            get(assign_zone_ownership).post(assign_zone_ownership),
        )
        .layer(from_fn(require_admin))
}

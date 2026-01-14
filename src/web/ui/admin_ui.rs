use crate::db::entities;
use crate::web::utils::Urls;
use crate::web::{GoatState, GoatStateTrait};
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::http::Uri;
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

use super::check_logged_in;

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
pub(crate) async fn dashboard(mut session: Session) -> Result<AdminUITemplate, Redirect> {
    let user = check_logged_in(&mut session, Uri::from_static(Urls::Home.as_ref())).await?;

    Ok(AdminUITemplate {
        user_is_admin: user.admin,
    })
}

pub(crate) async fn report_unowned_records(
    mut session: Session,
    State(state): State<GoatState>,
) -> Result<AdminReportUnownedRecords, Redirect> {
    let user = check_logged_in(&mut session, Uri::from_static(Urls::Home.as_ref())).await?;

    let txn = state.get_db_txn().await.map_err(|err| {
        error!("Failed to get DB connection: {err:?}");
        Redirect::to(Urls::Dashboard.as_ref())
    })?;

    // Get all zone IDs that have ownership records
    let owned_zone_ids: Vec<Uuid> = entities::ownership::Entity::find()
        .select_only()
        .column(entities::ownership::Column::Zoneid)
        .into_tuple()
        .all(&txn)
        .await
        .map_err(|err| {
            error!("Failed to query ownership: {err:?}");
            Redirect::to(Urls::Admin.as_ref())
        })?;

    // Get zones that don't have ownership records
    let unowned_zones = entities::zones::Entity::find()
        .filter(entities::zones::Column::Id.is_not_in(owned_zone_ids))
        .all(&txn)
        .await
        .map_err(|err| {
            error!("Failed to get unowned zones: {err:?}");
            Redirect::to(Urls::Admin.as_ref())
        })?;

    Ok(AdminReportUnownedRecords {
        user_is_admin: user.admin,
        records: vec![], // Records report not currently implemented
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
    mut session: Session,
    State(state): State<GoatState>,
    Path(id): Path<Uuid>,
    Form(form): Form<AssignOwnershipForm>,
) -> Result<AssignOwnershipTemplate, Redirect> {
    let current_user = check_logged_in(&mut session, Uri::from_static(Urls::Home.as_ref())).await?;

    let txn = state.get_db_txn().await.map_err(|err| {
        error!("Failed to get DB transaction: {err:?}");
        Redirect::to(Urls::Admin.as_ref())
    })?;

    let zone = entities::zones::Entity::find_by_id(id)
        .one(&txn)
        .await
        .map_err(|err| {
            error!("Failed to get zone by ID: {err:?}");
            Redirect::to(Urls::Admin.as_ref())
        })?
        .ok_or_else(|| {
            error!("No zone found with ID: {}", id);
            Redirect::to(Urls::Admin.as_ref())
        })?;

    if let Some(username) = form.username.as_ref() {
        debug!("Got a username in the form!");
        let target_user = entities::users::Entity::find()
            .filter(entities::users::Column::Username.eq(username.as_str()))
            .one(&txn)
            .await
            .map_err(|err| {
                error!("Failed to get user by name: {err:?}");
                Redirect::to(Urls::Admin.as_ref())
            })?;

        if let Some(target_user) = target_user {
            // Create ownership record
            let ownership = entities::ownership::ActiveModel {
                id: sea_orm::ActiveValue::NotSet,
                zoneid: Set(zone.id),
                userid: Set(target_user.id),
            };

            ownership.insert(&txn).await.map_err(|err| {
                error!("Failed to insert zone ownership: {err:?}");
                Urls::Admin.redirect_with_query(
                    [("message", format!("Failed to assign ownership: {err:?}"))]
                        .into_iter()
                        .collect(),
                )
            })?;

            txn.commit().await.map_err(|err| {
                error!("Failed to commit transaction: {err:?}");
                Urls::Admin.redirect()
            })?;

            return Err(Urls::Admin.redirect_with_query(
                [(
                    "message",
                    format!("Ownership assigned to {}", target_user.displayname),
                )]
                .into_iter()
                .collect(),
            ));
        }
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
}

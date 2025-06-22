use crate::db::{DBEntity, User, ZoneOwnership};
use crate::web::utils::Urls;
use crate::web::GoatState;
use crate::zones::FileZone;
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::http::Uri;
use axum::response::Redirect;
use axum::routing::get;
use axum::{Form, Router};
use serde::Deserialize;
use sqlx::Row;
use tower_sessions::Session;
use tracing::{debug, error};

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

    pub zones: Vec<FileZone>,
}

#[allow(dead_code)] // because this is only used in a template
/// Template struct for showing a zone record
pub(crate) struct ZoneRecord {
    id: u32,
    name: String,
    zoneid: u32,
}

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

    let mut pool = state.read().await.connpool.acquire().await.map_err(|err| {
        error!("Failed to get DB connection: {err:?}");
        Redirect::to(Urls::Dashboard.as_ref())
    })?;

    let rows = match sqlx::query(
        "select records_merged.record_id as id, records_merged.* from records_merged
        left join ownership
        ON records_merged.record_id = ownership.id
        where ownership.id is NULL",
    )
    .fetch_all(&mut *pool)
    .await
    {
        Ok(val) => {
            debug!("Got the rows!");
            val
        }
        Err(err) => {
            error!("Failed to query records from DB: {err:?}");
            vec![]
        }
    };

    debug!("starting to do the rows!");

    let records: Vec<ZoneRecord> = rows
        .iter()
        .map(|r| ZoneRecord {
            id: r.get("id"),
            name: r.get("name"),
            zoneid: r.get("zoneid"),
        })
        .collect();

    Ok(AdminReportUnownedRecords {
        user_is_admin: user.admin,
        records,
        zones: FileZone::get_unowned(&mut pool).await.map_err(|err| {
            error!("Failed to get unowned zones: {err:?}");
            Redirect::to(Urls::Admin.as_ref())
        })?,
    })
}

#[derive(Template, WebTemplate)]
#[template(path = "admin_ownership_template.html")]
pub(crate) struct AssignOwnershipTemplate {
    user_is_admin: bool,
    zone: Box<FileZone>,
    user: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AssignOwnershipForm {
    // userid: Option<i64>,
    username: Option<String>,
}

pub(crate) async fn assign_zone_ownership(
    mut session: Session,
    State(state): State<GoatState>,
    Path(id): Path<i64>,
    Form(form): Form<AssignOwnershipForm>,
) -> Result<AssignOwnershipTemplate, Redirect> {
    let user = check_logged_in(&mut session, Uri::from_static(Urls::Home.as_ref())).await?;

    let mut txn = state.read().await.connpool.begin().await.map_err(|err| {
        error!("Failed to start transaction: {err:?}");
        Redirect::to(Urls::Admin.as_ref())
    })?;

    let zone = FileZone::get(&state.read().await.connpool, id)
        .await
        .map_err(|err| {
            error!("Failed to get zone by ID: {err:?}");
            Redirect::to(Urls::Admin.as_ref())
        })?;

    if let Some(username) = form.username.as_ref() {
        debug!("Got a username in the form!");
        let user = User::get_by_name(&mut txn, username.as_str())
            .await
            .map_err(|err| {
                error!("Failed to get user by name: {err:?}");
                Redirect::to(Urls::Admin.as_ref())
            })?;
        drop(txn);
        if let Some(user) = user {
            if let (Some(userid), Some(zoneid)) = (user.id, zone.id) {
                // woo we found a valid user!
                ZoneOwnership {
                    id: None,
                    zoneid,
                    userid,
                }
                .save(&state.read().await.connpool)
                .await
                .map_err(|err| {
                    error!("Failed to insert zone ownership: {err:?}");
                    Redirect::to(Urls::Admin.as_ref())
                })?;
                return Err(Redirect::to(Urls::Admin.as_ref()));
            }
        }
    }

    Ok(AssignOwnershipTemplate {
        user_is_admin: user.admin,
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
            "/zones/assign_ownership/:id",
            get(assign_zone_ownership).post(assign_zone_ownership),
        )
}

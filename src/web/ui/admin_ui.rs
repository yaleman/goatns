use crate::web::utils::Urls;
use crate::web::GoatState;
use askama::Template;
use axum::http::Uri;
use axum::response::Redirect;
use axum::routing::get;
use axum::Router;
use sqlx::Row;
use tower_sessions::Session;

use super::check_logged_in;

#[derive(Template)]
#[template(path = "admin_ui.html")]
pub(crate) struct AdminUITemplate /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
}

#[derive(Template)]
#[template(path = "admin_report_unowned_records.html")]
pub(crate) struct AdminReportUnownedRecords /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
    pub records: Vec<ZoneRecord>,
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
    axum::extract::State(state): axum::extract::State<GoatState>,
) -> Result<AdminReportUnownedRecords, Redirect> {
    let user = check_logged_in(&mut session, Uri::from_static(Urls::Home.as_ref())).await?;

    let mut pool = state.read().await.connpool.acquire().await.map_err(|err| {
        log::error!("Failed to get DB connection: {err:?}");
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
            log::debug!("Got the rows!");
            val
        }
        Err(err) => {
            log::error!("Failed to query records from DB: {err:?}");
            vec![]
        }
    };

    log::debug!("starting to do the rows!");

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
    })
}

/// Build the router for user settings
pub fn router() -> Router<GoatState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/reports/unowned_records", get(report_unowned_records))
}

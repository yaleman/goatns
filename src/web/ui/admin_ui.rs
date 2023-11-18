use crate::web::GoatState;
use askama::Template;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use http::{Response, Uri};
use sqlx::Row;
use std::str::FromStr;
use tower_sessions::Session;

use super::check_logged_in;

#[derive(Template)]
#[template(path = "admin_ui.html")]
struct AdminUITemplate /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
}

#[derive(Template)]
#[template(path = "admin_report_unowned_records.html")]
struct AdminReportUnownedRecords /*<'a>*/ {
    // name: &'a str,
    pub user_is_admin: bool,
    pub records: Vec<ZoneRecord>,
}

#[allow(dead_code)] // because this is only used in a template
/// Template struct for showing a zone record
struct ZoneRecord {
    id: u32,
    name: String,
    zoneid: u32,
}

pub async fn dashboard(mut session: Session) -> impl IntoResponse {
    let user = match check_logged_in(&mut session, Uri::from_str("/").unwrap()).await {
        Ok(val) => val,
        Err(err) => return err.into_response(),
    };

    let context = AdminUITemplate {
        user_is_admin: user.admin,
    };
    // Html::from()).into_response()
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

#[axum::debug_handler]
pub async fn report_unowned_records(
    mut session: Session,
    axum::extract::State(state): axum::extract::State<GoatState>,
) -> impl IntoResponse {
    let user = match check_logged_in(&mut session, Uri::from_str("/").unwrap()).await {
        Ok(val) => val,
        Err(err) => return err.into_response(),
    };

    let mut pool = state.read().await.connpool.acquire().await.unwrap();

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

    let context = AdminReportUnownedRecords {
        user_is_admin: user.admin,
        records,
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

/// Build the router for user settings
pub fn router() -> Router<GoatState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/reports/unowned_records", get(report_unowned_records))
}

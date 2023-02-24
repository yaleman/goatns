use std::str::FromStr;

use askama::Template;
use axum::Router;
use axum::response::IntoResponse;
use axum::routing::get;
use axum_sessions::extractors::WritableSession;
use http::{Response, Uri};
use crate::web::GoatState;

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
}

pub async fn dashboard(
    mut session: WritableSession,
) -> impl IntoResponse {
    let user = match check_logged_in(&mut session, Uri::from_str("/").unwrap()).await {
        Ok(val) => val,
        Err(err) =>  return err.into_response(),
    };

    let context = AdminUITemplate {
        user_is_admin: user.admin
    };
    // Html::from()).into_response()
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

pub async fn report_unowned_records(
    mut session: WritableSession,
) -> impl IntoResponse {
    let user = match check_logged_in(&mut session, Uri::from_str("/").unwrap()).await {
        Ok(val) => val,
        Err(err) =>  return err.into_response(),
    };

    let context = AdminReportUnownedRecords {
        user_is_admin: user.admin
    };
    // Html::from()).into_response()
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

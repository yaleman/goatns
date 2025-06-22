use super::*;
use crate::enums::ContactDetails;

use axum::extract::{Query, State};
use serde::Deserialize;

pub async fn status() -> String {
    STATUS_OK.to_string()
}

#[derive(Template, WebTemplate)]
#[template(path = "index.html")]
pub(crate) struct IndexTemplate {
    admin_contact: String,
    error: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
/// If you want to be able to catch error or messages from the query string
pub(crate) struct QueryErrorOrMessage {
    pub(crate) error: Option<String>,
    pub(crate) message: Option<String>,
}

pub(crate) async fn index(
    State(state): State<GoatState>,
    Query(query): Query<QueryErrorOrMessage>,
) -> Result<IndexTemplate, ()> {
    let admin_contact = match state.read().await.config.admin_contact {
        ContactDetails::None => "".to_string(),
        _ => {
            format!(
                "- instance cared for by {}",
                state.read().await.config.admin_contact
            )
        }
    };
    Ok(IndexTemplate {
        admin_contact,
        error: query.error,
        message: query.message,
    })
}

use super::*;
use crate::enums::ContactDetails;
use askama::Template;
use axum::response::Html;
use axum_macros::debug_handler;

pub async fn status() -> String {
    STATUS_OK.to_string()
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    admin_contact: String,
}

#[debug_handler]
pub async fn index(Extension(state): Extension<SharedState>) -> Result<Html<String>, ()> {
    let admin_contact = state.config().await.admin_contact;
    let admin_contact = match admin_contact {
        ContactDetails::None => "".to_string(),
        _ => {
            format!("- instance cared for by {}", admin_contact.to_string())
        }
    };
    let context = IndexTemplate { admin_contact };
    Ok(Html::from(context.render().unwrap()))
}

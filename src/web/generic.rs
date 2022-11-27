use super::*;
use crate::enums::ContactDetails;
use askama::Template;
use axum::response::Html;
// use axum_macros::debug_handler;

pub async fn status() -> String {
    STATUS_OK.to_string()
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    admin_contact: String,
}

// #[debug_handler]
pub async fn index(
    axum::extract::State(state): axum::extract::State<GoatState>,
) -> Result<Html<String>, ()> {
    let admin_contact = match state.read().await.config.admin_contact {
        ContactDetails::None => "".to_string(),
        _ => {
            format!(
                "- instance cared for by {}",
                state.read().await.config.admin_contact.to_string()
            )
        }
    };
    let context = IndexTemplate { admin_contact };
    Ok(Html::from(context.render().unwrap()))
}

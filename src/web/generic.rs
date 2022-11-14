use super::*;

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
    // let admin_contact = match state.config().await.admin_contact {
    //     None => "".to_string(),
    //     Some(contact) => match ContactDetails::try_from(contact) {
    //         Ok(value) => format!("This instance cared for by {}", value.to_string()),
    //         Err(_) => "".to_string(),
    //     },
    // };
    let context = IndexTemplate {
        admin_contact: state.config().await.admin_contact.to_string(),
    };
    Ok(Html::from(context.render().unwrap()))
}

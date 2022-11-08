use super::*;

use askama::Template;
use axum::response::Html;

pub async fn status() -> String {
    STATUS_OK.to_string()
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate /*<'a>*/ {
    // name: &'a str,
}

pub async fn index() -> Result<Html<String>, ()> {
    let context = IndexTemplate {};
    Ok(Html::from(context.render().unwrap()))
}

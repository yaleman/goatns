use askama::Template;
use axum::response::Html;
use axum::Extension;

use crate::web::SharedState;

#[derive(Template)]
#[template(path = "user_settings.html")]
struct Settings;

/// The user settings page at /ui/settings
pub async fn settings(Extension(_state): Extension<SharedState>) -> Html<String> {
    let context = Settings;

    Html::from(context.render().unwrap())
}

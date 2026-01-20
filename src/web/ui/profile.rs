//! User profile things
//!

use crate::db::entities;
use crate::web::ui::check_logged_in;
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::OriginalUri;
use axum::response::Redirect;
use tower_sessions::Session;

#[derive(Template, WebTemplate)]
#[template(path = "view_profile.html")]
pub(crate) struct UserProfilePage {
    pub user: entities::users::Model,
    pub user_is_admin: bool,
}

pub(crate) async fn user_profile_get(
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> Result<UserProfilePage, Redirect> {
    // check_logged_in!(state, session, path);

    let user = check_logged_in(&mut session, path).await?;
    Ok(UserProfilePage {
        user_is_admin: user.admin,
        user,
    })
}

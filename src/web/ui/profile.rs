//! User profile things
//!

use askama::Template;
use axum::extract::{OriginalUri, State};
use axum::response::Redirect;
use tower_sessions::Session;

use crate::db::User;
use crate::web::utils::redirect_to_login;
use crate::web::GoatState;

#[derive(Template)]
#[template(path = "view_profile.html")]
pub(crate) struct UserProfilePage {
    pub user: User,
    pub user_is_admin: bool,
}

#[axum::debug_handler]
pub(crate) async fn user_profile_get(
    State(_state): State<GoatState>,
    session: Session,
    OriginalUri(_path): OriginalUri,
) -> Result<UserProfilePage, Redirect> {
    // check_logged_in!(state, session, path);

    let user: User = match session.get("user").await.unwrap() {
        Some(val) => {
            log::info!("Current user: {val:?}");
            val
        }
        None => return Err(redirect_to_login()),
    };
    Ok(UserProfilePage {
        user_is_admin: user.admin,
        user,
    })
}

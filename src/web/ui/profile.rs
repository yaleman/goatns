//! User profile things
//!

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{OriginalUri, State};
use axum::response::Redirect;
use tower_sessions::Session;

use crate::db::User;
use crate::web::ui::check_logged_in;

use crate::web::GoatState;

#[derive(Template, WebTemplate)]
#[template(path = "view_profile.html")]
pub(crate) struct UserProfilePage {
    pub user: User,
    pub user_is_admin: bool,
}

pub(crate) async fn user_profile_get(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> Result<UserProfilePage, Redirect> {
    // check_logged_in!(state, session, path);

    let user: User = check_logged_in(&mut session, path, state).await?;
    Ok(UserProfilePage {
        user_is_admin: user.admin,
        user,
    })
}

use crate::db::entities;
use crate::web::constants::SESSION_USER_KEY;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_sessions::Session;
use tracing::debug;

pub async fn require_admin(
    session: Session,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let user: Option<entities::users::Model> = match session.get(SESSION_USER_KEY).await {
        Ok(u) => u,
        Err(err) => {
            debug!("Session read error in admin middleware: {err:?}");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let user = match user {
        Some(u) => u,
        None => {
            debug!("No session user in admin middleware");
            session.clear().await;
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    if user.disabled {
        debug!("Disabled user {} attempted to access admin", user.id);
        session.clear().await;
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if !user.admin {
        debug!(
            "Non-admin user {} attempted to access admin area, denying",
            user.id
        );
        return StatusCode::FORBIDDEN.into_response();
    }

    next.run(req).await.into_response()
}

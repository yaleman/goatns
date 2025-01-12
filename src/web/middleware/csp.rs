use axum::{extract::State, http::HeaderValue, middleware::Next, response::Response};
use axum_csp::*;
use tracing::debug;

use crate::web::GoatState;

pub async fn cspheaders(
    State(state): State<GoatState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let uri: String = req.uri().path().to_string();
    let url_matcher: Option<CspUrlMatcher> = state.read().await.csp_matchers.iter().find_map(|c| {
        if c.matcher.is_match(&uri) {
            Some(c.to_owned())
        } else {
            None
        }
    });

    // wait for the middleware to come back
    let mut response = next.run(req).await;

    // if we found one, woot
    if let Some(rule) = url_matcher {
        let headers = response.headers_mut();
        if rule.matcher.is_match(&uri) {
            let header: HeaderValue = rule.into();
            headers.insert("Content-Security-Policy", header);
        }
    } else {
        debug!("didn't match uri");
    }

    response
}

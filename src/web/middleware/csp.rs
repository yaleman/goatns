use axum::middleware::Next;
use axum::response::Response;
use axum_csp::*;
use http::{HeaderValue, Request, StatusCode};

use crate::web::SharedState;

pub async fn cspheaders<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let uri: String = req.uri().path().to_string();
    let state: Option<&SharedState> = req.extensions().get();
    let url_matcher: Option<CspUrlMatcher> = match state {
        None => {
            log::error!("Couldn't get state in request :(");
            None
        }
        Some(state) => state.read().await.csp_matchers.iter().find_map(|c| {
            if c.matcher.is_match(&uri) {
                Some(c.to_owned())
            } else {
                None
            }
        }),
    };

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
        log::debug!("didn't match uri");
    }

    Ok(response)
}

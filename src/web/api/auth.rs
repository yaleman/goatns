use std::str::from_utf8;

use axum::middleware::Next;
use axum::response::Response;
use http::{Request, StatusCode};


pub async fn check_auth<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {

    let auth_header = match req.headers().get("Authorization"){
        None => return Err(StatusCode::UNAUTHORIZED),
        Some(val) => from_utf8(val.as_bytes()).unwrap(),
    };

    if !auth_header.starts_with("Bearer") {
        return Err(StatusCode::BAD_REQUEST)
    }

    let auth_token = match auth_header.split(' ').nth(1) {
        Some(val) => val,
        None => return Err(StatusCode::BAD_REQUEST)
    };

    log::debug!("Got auth header with bearer token: {auth_token:?}");
    // wait for the middleware to come back
    Ok(next.run(req).await)
}
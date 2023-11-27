use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use axum_csp::{CspDirective, CspDirectiveType, CspValue};
use tokio::io;

/// This is an example axum layer for implementing the axum-csp header enums
///
/// It just shoves a CSP header on /hello with the value `img-src: 'self' https:`
pub async fn cspheaders_layer<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let directive: CspDirective = CspDirective {
        directive_type: CspDirectiveType::ImgSrc,
        values: vec![CspValue::SelfSite, CspValue::SchemeHttps],
    };

    // wait for the middleware to come back
    let mut response = next.run(req).await;

    // add the header
    let headers = response.headers_mut();
    headers.insert(
        "Content-Security-Policy",
        HeaderValue::from_str(&directive.to_string()).unwrap(),
    );

    Ok(response)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    async fn home() {
        println!("Someone accessed /");
    }
    async fn hello() {
        println!("Someone accessed /hello");
    }

    let router = Router::new()
        .route("/hello", get(hello))
        .layer(from_fn(cspheaders_layer)) // everything already added will get the header
        .route("/", get(home));

    println!("Starting server on 127.0.0.1:6969");
    let _server = axum_server::bind(SocketAddr::from_str("127.0.0.1:6969").unwrap())
        .serve(router.into_make_service())
        .await;

    Ok(())
}

use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use axum_csp::{CspDirective, CspDirectiveType, CspValue};
use tokio::io;

/// This is an example axum layer for implementing the axum-csp header enums
///
/// It just shoves a CSP header on /hello with the value `img-src: 'self' https:`
pub async fn cspheaders_layer(
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
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
    async fn home() -> String {
        println!("Someone accessed /");
        "Home".to_string()
    }
    async fn hello() -> String {
        println!("Someone accessed /hello");
        "hello world".to_string()
    }

    let router = Router::new()
        .route("/hello", get(hello))
        .layer(from_fn(cspheaders_layer)) // everything already added will get the header
        .route("/", get(home));

    println!("Starting server on 127.0.0.1:6969");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:6969").await?;
    axum::serve(listener, router).await.unwrap();

    Ok(())
}

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::middleware::{from_fn, Next};
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use axum_csp::{CspDirective, CspDirectiveType, CspUrlMatcher, CspValue};
use http::{HeaderValue, Request, StatusCode};
use regex::RegexSet;
use tokio::io;
use tokio::sync::RwLock;

type SharedState = Arc<RwLock<State>>;

struct State {
    csp_matchers: Vec<CspUrlMatcher>,
}

/// This is an example axum layer for implementing the axum-csp header bits
///
/// It uses shared state to store a vec of matchers to check for URLs. yes, it's double-handling
/// the routing system, but I'm a terrible person with reasons, and it's from the GoatNS project
pub async fn cspheaders_layer<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let uri: String = req.uri().path().to_string();
    let state: Option<&SharedState> = req.extensions().get();
    let url_matcher: Option<CspUrlMatcher> = match state {
        None => {
            eprintln!("Couldn't get state in request :(");
            None
        }
        // see if we can find a match for the URL in the request
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
        eprintln!("didn't match uri");
    }

    Ok(response)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let csp_matchers = vec![CspUrlMatcher {
        matcher: RegexSet::new([r#"/hello"#]).unwrap(),
        directives: vec![CspDirective::from(
            CspDirectiveType::DefaultSrc,
            vec![CspValue::SelfSite],
        )],
    }];

    async fn home() {}
    async fn hello() {}

    let state = Arc::new(RwLock::new(State { csp_matchers }));

    let router = Router::new()
        .route("/", get(home))
        .route("/hello", get(hello))
        .layer(from_fn(cspheaders_layer))
        .layer(Extension(state));

    // start the server
    println!("Starting server on 127.0.0.1:6969");
    let _server = axum_server::bind(SocketAddr::from_str("127.0.0.1:6969").unwrap())
        .serve(router.into_make_service())
        .await;

    Ok(())
}

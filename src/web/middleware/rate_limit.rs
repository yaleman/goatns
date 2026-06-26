use axum::extract::ConnectInfo;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use governor::clock::DefaultClock;
use governor::middleware::NoOpMiddleware;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use nonzero_ext::nonzero;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;

const PER_SECOND: u32 = 100;
const BURST: u32 = 10;

#[derive(Clone)]
pub struct RateLimitState {
    limiter: Arc<RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock, NoOpMiddleware>>,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitState {
    pub fn new() -> Self {
        let quota = Quota::per_second(nonzero!(PER_SECOND)).allow_burst(nonzero!(BURST));
        let limiter = RateLimiter::keyed(quota);
        Self {
            limiter: Arc::new(limiter),
        }
    }
}

#[derive(Debug)]
struct RateLimited;

impl IntoResponse for RateLimited {
    fn into_response(self) -> Response {
        (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response()
    }
}

fn peer_ip(addr: &SocketAddr) -> IpAddr {
    addr.ip()
}

pub async fn rate_limit_middleware(
    State(gs): State<crate::web::GoatState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let ip = peer_ip(&addr);
    let rl = gs.read().await.rate_limit_state.clone();
    match rl.limiter.check_key(&ip) {
        Ok(()) => next.run(req).await,
        Err(_) => RateLimited.into_response(),
    }
}

pub fn rate_limit_state() -> RateLimitState {
    RateLimitState::new()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn rate_limit_allows_burst() {
        let state = RateLimitState::new();
        let ip: IpAddr = "127.0.0.1".parse().expect("loopback");
        for _ in 0..BURST {
            assert!(state.limiter.check_key(&ip).is_ok(), "burst should pass");
        }
    }

    #[test]
    fn rate_limit_blocks_excess_burst() {
        let state = RateLimitState::new();
        let ip: IpAddr = "127.0.0.1".parse().expect("loopback");
        for _ in 0..=BURST {
            let _ = state.limiter.check_key(&ip);
        }
        assert!(
            state.limiter.check_key(&ip).is_err(),
            "over-burst should be blocked"
        );
    }

    #[test]
    fn rate_limit_per_ip_isolation() {
        let state = RateLimitState::new();
        let ip_a: IpAddr = "10.0.0.1".parse().expect("a");
        let ip_b: IpAddr = "10.0.0.2".parse().expect("b");
        for _ in 0..=BURST {
            let _ = state.limiter.check_key(&ip_a);
        }
        assert!(state.limiter.check_key(&ip_a).is_err(), "a exhausted");
        assert!(state.limiter.check_key(&ip_b).is_ok(), "b unaffected");
    }
}

//! What one visitor may ask the site for.
//!
//! The counting is [`phonix_limit`]; this is the policy over it. There is very
//! little policy, and that is the point - see ADR 0007 section 6. The server
//! sorts requests into four tiers because a password being guessed at and a
//! database being created deserve different ceilings. Every request here is a
//! page.
//!
//! # What is not counted
//!
//! **The two assets.** One page view is the HTML plus a stylesheet plus a
//! script, so counting all three would spend a reader's allowance three times
//! as fast as the configured number suggests. In production nginx serves them
//! and they never arrive here; in development they do.
//!
//! **The health probe.** An orchestrator polls it on a schedule and must never
//! be told to come back later.
//!
//! # The key
//!
//! `[security.rate_limit] client_ip_header`, reused rather than repeated. One
//! source and no fallback chain between headers: a chain is a bypass, because
//! the caller picks which link answers by omitting the ones above it. With
//! nothing configured the key is the peer address of the connection, which is
//! unforgeable and correct whenever this process is reachable directly.

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, StatusCode, header};
use phonix_limit::Decision;

use crate::state::SiteState;

/// Refuse a visitor asking for far more than a visitor asks for.
pub async fn enforce(State(state): State<SiteState>, request: Request, next: Next) -> Response {
    let config = &state.site().rate_limit;

    if !config.enabled || !is_counted(request.uri().path()) {
        return next.run(request).await;
    }

    let key = client_key(&state, request.headers(), &request);
    let window = Duration::from_secs(config.window_secs);

    match state.limiter().check(&key, config.requests, window) {
        Decision::Allow => next.run(request).await,
        Decision::Refuse { retry_after_secs } => {
            // At warn: nobody reading a five-page site reaches this, so it is
            // either an impolite crawler or a bug in this application.
            tracing::warn!(
                client = %key,
                path = %request.uri().path(),
                "rate limit exceeded"
            );

            // Plain text rather than a rendered page. Rendering one would spend
            // more of this server on the refusal than on the request refused.
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(
                    header::RETRY_AFTER,
                    header::HeaderValue::from_str(&retry_after_secs.to_string())
                        .unwrap_or(header::HeaderValue::from_static("1")),
                )],
                "Too many requests. Try again shortly.",
            )
                .into_response()
        }
    }
}

/// Whether this path counts against the allowance.
fn is_counted(path: &str) -> bool {
    !path.starts_with("/assets/") && path != "/health" && path != "/robots.txt"
}

/// What to count this request against.
fn client_key(state: &SiteState, headers: &HeaderMap, request: &Request) -> String {
    if let Some(name) = state.config.security.rate_limit.ip_header()
        && let Some(value) = headers.get(&name).and_then(|value| value.to_str().ok())
    {
        let value = value.trim();
        if !value.is_empty() {
            // Bounded: this becomes a map key, and an unbounded header should
            // not decide how much memory one request costs.
            return value.chars().take(64).collect();
        }
    }

    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        // The port is dropped: it changes on every connection, so keying on it
        // would hand each request a fresh allowance and count nothing.
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_view_costs_one_rather_than_three() {
        assert!(is_counted("/"));
        assert!(is_counted("/pricing"));

        // The stylesheet and the script come with every page. Counting them
        // would make the configured number mean a third of what it says.
        assert!(!is_counted("/assets/site.abcdef123456.css"));
        assert!(!is_counted("/assets/site.abcdef123456.js"));
    }

    /// An orchestrator polls on a schedule and must never be told to wait.
    #[test]
    fn the_health_probe_is_never_refused() {
        assert!(!is_counted("/health"));
    }
}

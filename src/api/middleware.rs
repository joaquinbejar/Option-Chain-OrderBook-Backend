//! API middleware: JWT authentication, per-route permission enforcement, and
//! sliding-window rate limiting keyed by the JWT `sub`.

use crate::auth::RateLimitDecision;
use crate::error::ApiError;
use crate::models::Permission;
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderValue, Method, Request, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default per-subject rate limit (requests per sliding 60s window).
const DEFAULT_RATE_LIMIT: u32 = 100;

/// Tighter rate limit (per client IP) for the unauthenticated token endpoint.
const TOKEN_ISSUE_RATE_LIMIT: u32 = 10;

/// Health-check path (exempt from authentication).
const HEALTH_PATH: &str = "/health";

/// Token-issuance path (exempt from authentication, IP rate-limited).
const TOKEN_PATH: &str = "/api/v1/auth/token";

/// Returns the current Unix time in seconds.
#[inline]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Authentication + authorization + rate-limit middleware.
///
/// Extracts a JWT from `Authorization: Bearer <jwt>` (REST and non-browser WS) or
/// the `?token=<jwt>` query parameter (the browser `/ws` upgrade), verifies it,
/// enforces the per-route required [`Permission`], rate-limits by the JWT `sub`,
/// and injects [`Claims`](crate::auth::Claims) into the request extensions. `/health` and
/// `POST /api/v1/auth/token` are exempt (the latter is IP rate-limited).
///
/// Returns `401` for a missing/invalid/expired token, `403` for insufficient
/// permission, and `429` when the rate limit is exceeded.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Health check is always open.
    if path == HEALTH_PATH {
        return next.run(request).await;
    }

    // Token issuance is unauthenticated but rate-limited by the trusted peer
    // address (never a spoofable client header unless a proxy is trusted).
    if method == Method::POST && path == TOKEN_PATH {
        let key = token_rate_limit_key(&request, state.trust_proxy);
        let decision = state
            .auth
            .check_rate_limit_status(&key, TOKEN_ISSUE_RATE_LIMIT);
        if !decision.allowed {
            return rate_limited_response(TOKEN_ISSUE_RATE_LIMIT, decision);
        }
        let mut response = next.run(request).await;
        add_rate_limit_headers(&mut response, TOKEN_ISSUE_RATE_LIMIT, decision);
        return response;
    }

    // Extract and verify the token.
    let Some(token) = extract_token(&request) else {
        return ApiError::Unauthorized("missing authentication token".to_string()).into_response();
    };
    let claims = match state.auth.verify_token(&token) {
        Ok(claims) => claims,
        Err(err) => return err.into_response(),
    };

    // Rate limit by subject.
    let decision = state
        .auth
        .check_rate_limit_status(&claims.sub, DEFAULT_RATE_LIMIT);
    if !decision.allowed {
        return rate_limited_response(DEFAULT_RATE_LIMIT, decision);
    }

    // Enforce the per-route required permission.
    let required = required_permission(&method, &path);
    if !claims.has_permission(required) {
        return ApiError::Forbidden(format!("requires {required:?} permission")).into_response();
    }

    // Make the caller's claims available to handlers.
    request.extensions_mut().insert(claims);

    let mut response = next.run(request).await;
    add_rate_limit_headers(&mut response, DEFAULT_RATE_LIMIT, decision);
    response
}

/// Determines the [`Permission`] required for a method + path.
///
/// GETs require `Read`; mutations require `Trade`; market-maker controls, admin
/// snapshots, and underlying deletion require `Admin`.
fn required_permission(method: &Method, path: &str) -> Permission {
    // Admin-only subtrees.
    if path.starts_with("/api/v1/controls") || path.starts_with("/api/v1/admin") {
        return Permission::Admin;
    }
    // Deleting an underlying root (`/api/v1/underlyings/{underlying}`) is admin.
    if method == Method::DELETE && is_underlying_root(path) {
        return Permission::Admin;
    }
    match *method {
        Method::GET | Method::HEAD | Method::OPTIONS => Permission::Read,
        _ => Permission::Trade,
    }
}

/// Returns true if `path` is exactly `/api/v1/underlyings/{underlying}`
/// (no deeper segments).
fn is_underlying_root(path: &str) -> bool {
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    segments.len() == 4
        && segments[0] == "api"
        && segments[1] == "v1"
        && segments[2] == "underlyings"
        && !segments[3].is_empty()
}

/// Extracts a JWT from the `Authorization: Bearer` header or the `?token=` query
/// parameter (used by the browser WebSocket upgrade).
fn extract_token(request: &Request<Body>) -> Option<String> {
    if let Some(value) = request.headers().get(AUTHORIZATION)
        && let Ok(text) = value.to_str()
        && let Some(rest) = text.strip_prefix("Bearer ")
    {
        let token = rest.trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }

    // `?token=<jwt>` — JWTs are URL-safe (base64url + '.'), no decoding needed.
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("token=")
                && !value.is_empty()
            {
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Builds a `429 Too Many Requests` response carrying the limiter's real
/// window state (issue #62): `remaining` from the decision (always 0 when
/// denied) and `reset` = the actual expiry of the oldest in-window request.
fn rate_limited_response(limit: u32, decision: RateLimitDecision) -> Response {
    let now = now_secs();
    ApiError::RateLimitExceeded {
        limit,
        remaining: decision.remaining,
        reset: decision.reset_secs,
        retry_after: decision.reset_secs.saturating_sub(now),
    }
    .into_response()
}

/// Adds `X-RateLimit-*` headers reflecting the limiter's real window state
/// (issue #62). Header values are built with the infallible integer
/// `HeaderValue` conversions — no stringify-then-parse (issue #63).
fn add_rate_limit_headers(response: &mut Response, limit: u32, decision: RateLimitDecision) {
    let headers = response.headers_mut();
    headers.insert("X-RateLimit-Limit", HeaderValue::from(limit));
    headers.insert(
        "X-RateLimit-Remaining",
        HeaderValue::from(decision.remaining),
    );
    headers.insert("X-RateLimit-Reset", HeaderValue::from(decision.reset_secs));
}

/// Derives the rate-limit bucket key for the unauthenticated token endpoint.
///
/// By default the identity is the trusted socket peer address provided by axum's
/// [`ConnectInfo`] (wired via `into_make_service_with_connect_info` in
/// `main.rs`), so a client cannot influence its own bucket — this closes the
/// spoofable-`X-Forwarded-For` rate-limit bypass from issue #48. A
/// client-supplied `X-Forwarded-For` / `X-Real-IP` header is honored ONLY when
/// `trust_proxy` is enabled (the operator asserts a trusted reverse proxy
/// terminates the connection). The constant `"unknown"` shared bucket is never
/// used as a catch-all under normal operation.
fn token_rate_limit_key(request: &Request<Body>, trust_proxy: bool) -> String {
    if trust_proxy && let Some(ip) = forwarded_ip(request) {
        return format!("token_issue:fwd:{ip}");
    }

    match request.extensions().get::<ConnectInfo<SocketAddr>>() {
        Some(ConnectInfo(addr)) => format!("token_issue:peer:{}", addr.ip()),
        None => {
            // ConnectInfo is always injected in production (see `main.rs`); its
            // absence means the service was built without connect-info wiring.
            // Apply the limit under a single fallback bucket and warn so the
            // misconfiguration is visible rather than silently un-limited.
            tracing::warn!(
                "token-endpoint rate limit could not resolve a peer address; \
                 using a fallback bucket (check into_make_service_with_connect_info)"
            );
            "token_issue:peer:unresolved".to_string()
        }
    }
}

/// Extracts a client IP from `X-Forwarded-For` (first hop) or `X-Real-IP`.
///
/// Only consulted when the immediate peer is a configured trusted proxy
/// (`trust_proxy`); these headers are client-controlled and must never be
/// trusted for rate-limit identity by default.
fn forwarded_ip(request: &Request<Body>) -> Option<String> {
    if let Some(forwarded) = request.headers().get("X-Forwarded-For")
        && let Ok(value) = forwarded.to_str()
        && let Some(first) = value.split(',').next()
    {
        let ip = first.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }

    if let Some(real_ip) = request.headers().get("X-Real-IP")
        && let Ok(value) = real_ip.to_str()
    {
        let ip = value.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;

    fn req(method: Method, uri: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("request builds")
    }

    /// Builds a request with a `ConnectInfo<SocketAddr>` extension, mimicking
    /// axum's `into_make_service_with_connect_info` wiring.
    fn req_with_peer(method: Method, uri: &str, peer: &str) -> Request<Body> {
        let addr: SocketAddr = peer.parse().expect("valid socket addr");
        let mut request = req(method, uri);
        request.extensions_mut().insert(ConnectInfo(addr));
        request
    }

    #[test]
    fn test_forwarded_ip_xff_first_hop() {
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "192.168.1.1, 10.0.0.1")
            .body(Body::empty())
            .expect("request builds");
        assert_eq!(forwarded_ip(&request), Some("192.168.1.1".to_string()));
    }

    #[test]
    fn test_forwarded_ip_real_ip() {
        let request = Request::builder()
            .uri("/test")
            .header("X-Real-IP", "192.168.1.2")
            .body(Body::empty())
            .expect("request builds");
        assert_eq!(forwarded_ip(&request), Some("192.168.1.2".to_string()));
    }

    #[test]
    fn test_forwarded_ip_absent() {
        let request = req(Method::GET, "/test");
        assert_eq!(forwarded_ip(&request), None);
    }

    #[test]
    fn test_token_key_uses_peer_addr_by_default() {
        // With trust_proxy off, the key derives from the socket peer, NOT the
        // (spoofable) forwarding header.
        let mut request = req_with_peer(Method::POST, TOKEN_PATH, "203.0.113.7:54321");
        request
            .headers_mut()
            .insert("X-Forwarded-For", "1.2.3.4".parse().expect("header value"));
        assert_eq!(
            token_rate_limit_key(&request, false),
            "token_issue:peer:203.0.113.7"
        );
    }

    #[test]
    fn test_token_key_forged_xff_does_not_change_bucket() {
        // Two requests from the same peer but with different forged XFF headers
        // must land in the SAME bucket when proxies are not trusted.
        let mut a = req_with_peer(Method::POST, TOKEN_PATH, "203.0.113.7:1111");
        a.headers_mut()
            .insert("X-Forwarded-For", "9.9.9.9".parse().expect("header value"));
        let mut b = req_with_peer(Method::POST, TOKEN_PATH, "203.0.113.7:2222");
        b.headers_mut()
            .insert("X-Forwarded-For", "8.8.8.8".parse().expect("header value"));
        assert_eq!(
            token_rate_limit_key(&a, false),
            token_rate_limit_key(&b, false)
        );
    }

    #[test]
    fn test_token_key_distinct_peers_distinct_buckets() {
        let a = req_with_peer(Method::POST, TOKEN_PATH, "203.0.113.7:1111");
        let b = req_with_peer(Method::POST, TOKEN_PATH, "198.51.100.2:1111");
        assert_ne!(
            token_rate_limit_key(&a, false),
            token_rate_limit_key(&b, false)
        );
    }

    #[test]
    fn test_token_key_honors_proxy_when_trusted() {
        // With trust_proxy on, the forwarded header is honored.
        let mut request = req_with_peer(Method::POST, TOKEN_PATH, "203.0.113.7:54321");
        request
            .headers_mut()
            .insert("X-Forwarded-For", "1.2.3.4".parse().expect("header value"));
        assert_eq!(
            token_rate_limit_key(&request, true),
            "token_issue:fwd:1.2.3.4"
        );
    }

    #[test]
    fn test_token_key_trusted_proxy_without_header_falls_back_to_peer() {
        // trust_proxy on but no forwarding header present: fall back to the peer.
        let request = req_with_peer(Method::POST, TOKEN_PATH, "203.0.113.7:54321");
        assert_eq!(
            token_rate_limit_key(&request, true),
            "token_issue:peer:203.0.113.7"
        );
    }

    #[test]
    fn test_token_key_without_connect_info_uses_fallback_bucket() {
        // No ConnectInfo extension (e.g. service built without connect-info): a
        // single fallback bucket is used rather than a constant "unknown".
        let request = req(Method::POST, TOKEN_PATH);
        assert_eq!(
            token_rate_limit_key(&request, false),
            "token_issue:peer:unresolved"
        );
    }

    #[test]
    fn test_extract_token_bearer_header() {
        let request = Request::builder()
            .uri("/api/v1/stats")
            .header(AUTHORIZATION, "Bearer abc.def.ghi")
            .body(Body::empty())
            .expect("request builds");
        assert_eq!(extract_token(&request), Some("abc.def.ghi".to_string()));
    }

    #[test]
    fn test_extract_token_query_param() {
        let request = req(Method::GET, "/ws?foo=bar&token=abc.def.ghi");
        assert_eq!(extract_token(&request), Some("abc.def.ghi".to_string()));
    }

    #[test]
    fn test_extract_token_missing() {
        let request = req(Method::GET, "/api/v1/stats");
        assert_eq!(extract_token(&request), None);
    }

    #[test]
    fn test_required_permission_get_is_read() {
        assert_eq!(
            required_permission(&Method::GET, "/api/v1/stats"),
            Permission::Read
        );
        assert_eq!(
            required_permission(&Method::GET, "/api/v1/orders"),
            Permission::Read
        );
    }

    #[test]
    fn test_required_permission_mutation_is_trade() {
        assert_eq!(
            required_permission(
                &Method::POST,
                "/api/v1/underlyings/BTC/expirations/20251231/strikes/50000/options/call/orders"
            ),
            Permission::Trade
        );
        assert_eq!(
            required_permission(&Method::POST, "/api/v1/prices"),
            Permission::Trade
        );
    }

    #[test]
    fn test_required_permission_controls_is_admin() {
        assert_eq!(
            required_permission(&Method::GET, "/api/v1/controls"),
            Permission::Admin
        );
        assert_eq!(
            required_permission(&Method::POST, "/api/v1/controls/kill-switch"),
            Permission::Admin
        );
    }

    #[test]
    fn test_required_permission_admin_snapshots_is_admin() {
        assert_eq!(
            required_permission(&Method::POST, "/api/v1/admin/snapshot"),
            Permission::Admin
        );
        assert_eq!(
            required_permission(&Method::GET, "/api/v1/admin/snapshots"),
            Permission::Admin
        );
    }

    #[test]
    fn test_required_permission_underlying_delete_is_admin() {
        assert_eq!(
            required_permission(&Method::DELETE, "/api/v1/underlyings/BTC"),
            Permission::Admin
        );
        // Deleting an order is only a trade-level mutation.
        assert_eq!(
            required_permission(
                &Method::DELETE,
                "/api/v1/underlyings/BTC/expirations/20251231/strikes/50000/options/call/orders/1"
            ),
            Permission::Trade
        );
    }

    #[test]
    fn test_is_underlying_root() {
        assert!(is_underlying_root("/api/v1/underlyings/BTC"));
        assert!(!is_underlying_root("/api/v1/underlyings"));
        assert!(!is_underlying_root("/api/v1/underlyings/BTC/expirations"));
    }

    #[test]
    fn test_default_rate_limit_constant() {
        assert_eq!(DEFAULT_RATE_LIMIT, 100);
    }

    /// Issue #62: success-path headers carry the decision's REAL window state,
    /// not `limit - 1` / `now + 60`.
    #[test]
    fn test_add_rate_limit_headers_uses_decision_values() {
        let decision = RateLimitDecision {
            allowed: true,
            remaining: 42,
            reset_secs: 1_752_000_123,
        };
        let mut response = Response::new(Body::empty());
        add_rate_limit_headers(&mut response, DEFAULT_RATE_LIMIT, decision);

        let headers = response.headers();
        assert_eq!(
            headers.get("X-RateLimit-Limit"),
            Some(&HeaderValue::from(DEFAULT_RATE_LIMIT))
        );
        assert_eq!(
            headers.get("X-RateLimit-Remaining"),
            Some(&HeaderValue::from(42u32))
        );
        assert_eq!(
            headers.get("X-RateLimit-Reset"),
            Some(&HeaderValue::from(1_752_000_123u64))
        );
    }

    /// Issue #62: the 429 response reports the same decision values (remaining
    /// 0, real reset) so success and denial are consistent.
    #[test]
    fn test_rate_limited_response_uses_decision_values() {
        let reset_secs = now_secs() + 37;
        let decision = RateLimitDecision {
            allowed: false,
            remaining: 0,
            reset_secs,
        };
        let response = rate_limited_response(DEFAULT_RATE_LIMIT, decision);

        assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
        let headers = response.headers();
        assert_eq!(
            headers.get("X-RateLimit-Remaining"),
            Some(&HeaderValue::from_static("0"))
        );
        assert_eq!(
            headers.get("X-RateLimit-Reset"),
            Some(&HeaderValue::from(reset_secs))
        );
        // Retry-After is derived from the real reset, not a fixed 60.
        let retry_after: u64 = headers
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .expect("Retry-After header present and numeric");
        assert!(retry_after <= 37, "retry-after derives from the real reset");
    }
}

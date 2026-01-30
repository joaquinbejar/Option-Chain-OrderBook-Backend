//! API middleware for rate limiting and authentication.

use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Header name for API key.
pub const API_KEY_HEADER: &str = "X-API-Key";

/// Default rate limit for unauthenticated requests.
const DEFAULT_RATE_LIMIT: u32 = 100;

/// Anonymous key prefix for rate limiting unauthenticated requests.
const ANONYMOUS_KEY_PREFIX: &str = "anon_";

/// Rate limiting middleware.
///
/// Extracts API key from request headers and checks rate limits.
/// Returns 429 Too Many Requests if rate limit is exceeded.
/// Adds rate limit headers to all responses.
pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Exempt health check endpoint
    if path == "/health" {
        return next.run(request).await;
    }

    // Extract API key from header
    let api_key = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Determine rate limit and key_id
    let (key_id, rate_limit) = if let Some(ref key) = api_key {
        // Validate API key and get rate limit
        if let Some(stored_key) = state.api_key_store.validate_key(key) {
            (stored_key.key_id.clone(), stored_key.rate_limit)
        } else {
            // Invalid API key - use IP-based rate limiting with default limit
            let client_ip = extract_client_ip(&request);
            (
                format!("{}{}", ANONYMOUS_KEY_PREFIX, client_ip),
                DEFAULT_RATE_LIMIT,
            )
        }
    } else {
        // No API key - use IP-based rate limiting
        let client_ip = extract_client_ip(&request);
        (
            format!("{}{}", ANONYMOUS_KEY_PREFIX, client_ip),
            DEFAULT_RATE_LIMIT,
        )
    };

    // Check rate limit
    let allowed = state.api_key_store.check_rate_limit(&key_id, rate_limit);

    // Calculate rate limit info
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let reset = now + 60; // Reset in 60 seconds

    if !allowed {
        // Rate limit exceeded
        return ApiError::RateLimitExceeded {
            limit: rate_limit,
            remaining: 0,
            reset,
            retry_after: 60,
        }
        .into_response();
    }

    // Process request
    let mut response = next.run(request).await;

    // Add rate limit headers to response
    let headers = response.headers_mut();
    headers.insert("X-RateLimit-Limit", rate_limit.to_string().parse().unwrap());
    // Note: We don't track exact remaining count, so we estimate
    headers.insert(
        "X-RateLimit-Remaining",
        (rate_limit.saturating_sub(1)).to_string().parse().unwrap(),
    );
    headers.insert("X-RateLimit-Reset", reset.to_string().parse().unwrap());

    response
}

/// Extract client IP from request.
fn extract_client_ip(request: &Request<Body>) -> String {
    // Try X-Forwarded-For header first
    if let Some(forwarded) = request.headers().get("X-Forwarded-For")
        && let Ok(value) = forwarded.to_str()
        && let Some(ip) = value.split(',').next()
    {
        return ip.trim().to_string();
    }

    // Try X-Real-IP header
    if let Some(real_ip) = request.headers().get("X-Real-IP")
        && let Ok(value) = real_ip.to_str()
    {
        return value.to_string();
    }

    // Default to unknown
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;

    #[test]
    fn test_extract_client_ip_forwarded() {
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "192.168.1.1, 10.0.0.1")
            .body(Body::empty())
            .unwrap();

        let ip = extract_client_ip(&request);
        assert_eq!(ip, "192.168.1.1");
    }

    #[test]
    fn test_extract_client_ip_real_ip() {
        let request = Request::builder()
            .uri("/test")
            .header("X-Real-IP", "192.168.1.2")
            .body(Body::empty())
            .unwrap();

        let ip = extract_client_ip(&request);
        assert_eq!(ip, "192.168.1.2");
    }

    #[test]
    fn test_extract_client_ip_unknown() {
        let request = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let ip = extract_client_ip(&request);
        assert_eq!(ip, "unknown");
    }

    #[test]
    fn test_api_key_header_constant() {
        assert_eq!(API_KEY_HEADER, "X-API-Key");
    }

    #[test]
    fn test_default_rate_limit_constant() {
        assert_eq!(DEFAULT_RATE_LIMIT, 100);
    }

    #[test]
    fn test_anonymous_key_prefix() {
        assert_eq!(ANONYMOUS_KEY_PREFIX, "anon_");
    }
}

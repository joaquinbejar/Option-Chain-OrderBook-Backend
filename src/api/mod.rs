//! API routes module.

use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::warn;

pub mod controls;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod websocket;

pub use middleware::auth_middleware;
pub use routes::create_router;

/// Parses raw CORS origin strings into HTTP `HeaderValue`s for an allowlist.
///
/// Each entry is parsed into a `HeaderValue`; any entry that is not a valid HTTP
/// header value, or the wildcard `*` (which would defeat the allowlist), is
/// skipped with a `WARN`. Returns only the kept, valid origins.
#[must_use]
pub fn parse_origin_header_values(origins: &[String]) -> Vec<HeaderValue> {
    let mut out = Vec::with_capacity(origins.len());
    for origin in origins {
        if origin == "*" {
            warn!(
                origin = %origin,
                "skipping wildcard CORS origin; an explicit allowlist is required"
            );
            continue;
        }
        match HeaderValue::from_str(origin) {
            Ok(value) => out.push(value),
            Err(e) => warn!(origin = %origin, error = %e, "skipping invalid CORS origin"),
        }
    }
    out
}

/// Builds the CORS layer from an allowlist of origin strings.
///
/// Origins are restricted to the parsed allowlist (never `Any`). Methods and
/// headers stay explicit (GET/POST/PATCH/DELETE/OPTIONS and
/// `Authorization`/`Content-Type`). Credentials are intentionally NOT enabled:
/// authentication uses the `Authorization` bearer header, not cookies, so a
/// wildcard-with-credentials configuration can never arise.
pub fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let allowed = parse_origin_header_values(origins);
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_origin_header_values_keeps_valid_skips_invalid_and_wildcard() {
        let origins = vec![
            "http://localhost:5173".to_string(),
            "http://bad\norigin".to_string(), // control char -> invalid header value
            "*".to_string(),                  // wildcard -> skipped
            "http://127.0.0.1:5173".to_string(),
        ];

        let parsed = parse_origin_header_values(&origins);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], HeaderValue::from_static("http://localhost:5173"));
        assert_eq!(parsed[1], HeaderValue::from_static("http://127.0.0.1:5173"));
    }

    #[test]
    fn parse_origin_header_values_empty_input_is_empty() {
        assert!(parse_origin_header_values(&[]).is_empty());
    }
}

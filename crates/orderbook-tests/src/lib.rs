//! Integration tests for the Option Chain OrderBook API.
//!
//! These tests require the API server to be running. Configure the server URL
//! via the `API_BASE_URL` environment variable (default: `http://localhost:8080`).

use orderbook_client::{ClientConfig, Error, OrderbookClient, Permission, TokenRequest};
use std::time::Duration;

/// Gets the API base URL from environment or uses default.
#[must_use]
pub fn get_api_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Gets the operator bootstrap secret from the environment.
///
/// Mirrors the server's `AUTH_BOOTSTRAP_SECRET`; integration tests use it to mint
/// JWTs against a running server.
#[must_use]
pub fn get_bootstrap_secret() -> String {
    std::env::var("AUTH_BOOTSTRAP_SECRET").unwrap_or_else(|_| "test-bootstrap-secret".to_string())
}

/// Creates an unauthenticated test client (carries no token).
///
/// Useful for `/health` and for asserting `401` on protected endpoints.
///
/// # Errors
/// Returns error if client creation fails.
pub fn create_test_client() -> Result<OrderbookClient, Error> {
    OrderbookClient::new(ClientConfig {
        base_url: get_api_url(),
        timeout: Duration::from_secs(10),
        token: None,
    })
}

/// Obtains a JWT from the running server with the given permissions, using the
/// bootstrap secret from the environment.
///
/// # Errors
/// Returns error if the request fails or issuance is disabled/rejected.
pub async fn obtain_token(permissions: Vec<Permission>) -> Result<String, Error> {
    let client = create_test_client()?;
    let response = client
        .issue_token(&TokenRequest {
            secret: get_bootstrap_secret(),
            permissions,
            ttl_secs: Some(3600),
        })
        .await?;
    Ok(response.token)
}

/// Creates an authenticated test client carrying a freshly issued token with the
/// given permissions.
///
/// # Errors
/// Returns error if token issuance or client creation fails.
pub async fn create_authenticated_client(
    permissions: Vec<Permission>,
) -> Result<OrderbookClient, Error> {
    let token = obtain_token(permissions).await?;
    OrderbookClient::new(ClientConfig {
        base_url: get_api_url(),
        timeout: Duration::from_secs(10),
        token: Some(token),
    })
}

/// Generates a unique test symbol to avoid conflicts between tests.
#[must_use]
pub fn unique_symbol(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}_{}_{}", prefix, ts, counter)
}

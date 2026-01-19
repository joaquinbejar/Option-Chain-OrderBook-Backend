//! Integration tests for the Option Chain OrderBook API.
//!
//! These tests require the API server to be running. Configure the server URL
//! via the `API_BASE_URL` environment variable (default: `http://localhost:8080`).

use orderbook_client::{ClientConfig, OrderbookClient};
use std::time::Duration;

/// Gets the API base URL from environment or uses default.
#[must_use]
pub fn get_api_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Creates a test client configured for the API.
///
/// # Errors
/// Returns error if client creation fails.
pub fn create_test_client() -> Result<OrderbookClient, orderbook_client::Error> {
    OrderbookClient::new(ClientConfig {
        base_url: get_api_url(),
        timeout: Duration::from_secs(10),
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

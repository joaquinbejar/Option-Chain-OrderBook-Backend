//! Health check and statistics endpoint tests.

use orderbook_tests::{create_test_client, read_client};

#[tokio::test]
async fn test_health_check() {
    // `/health` is exempt from auth; an unauthenticated client reaches it.
    let client = create_test_client().expect("Failed to create client");

    let health = client.health_check().await.expect("Health check failed");

    assert_eq!(health.status, "healthy");
    assert!(!health.version.is_empty());
}

#[tokio::test]
async fn test_global_stats() {
    // `/api/v1/stats` requires a Read token.
    let client = read_client().await.expect("Failed to create read client");

    let stats = client
        .get_global_stats()
        .await
        .expect("Failed to get stats");

    // Stats should return valid counts (usize is always >= 0). The server is
    // provisioned with BTC/ETH/GOLD, so there is at least one underlying.
    assert!(stats.underlying_count >= 3);
    let _ = stats.total_expirations;
    let _ = stats.total_strikes;
    let _ = stats.total_orders;
}

//! Health check and status endpoint tests.

use orderbook_tests::create_test_client;

#[tokio::test]
async fn test_health_check() {
    let client = create_test_client().expect("Failed to create client");

    let health = client.health_check().await.expect("Health check failed");

    assert_eq!(health.status, "ok");
    assert!(!health.version.is_empty());
}

#[tokio::test]
async fn test_global_stats() {
    let client = create_test_client().expect("Failed to create client");

    let stats = client
        .get_global_stats()
        .await
        .expect("Failed to get stats");

    // Stats should return valid counts (usize is always >= 0)
    // Just verify we got a response with the expected fields
    let _ = stats.underlying_count;
    let _ = stats.total_expirations;
    let _ = stats.total_strikes;
    let _ = stats.total_orders;
}

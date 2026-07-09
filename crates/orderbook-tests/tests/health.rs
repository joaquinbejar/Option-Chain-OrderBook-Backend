//! Health check and statistics endpoint tests.

use orderbook_tests::{admin_client, create_test_client, unique_symbol};

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
    // Issue #86: mutate known state and assert the counts actually move —
    // this fails if the stats endpoint stops reflecting reality. Mutation
    // needs an Admin token (create/delete underlying).
    let client = admin_client().await.expect("Failed to create admin client");
    let symbol = unique_symbol("STATS");

    // Phase 1: act, capturing outcomes (capture-then-assert per issue #85).
    let before = client.get_global_stats().await;
    let created = client.create_underlying(&symbol).await;
    let during = client.get_global_stats().await;

    // Phase 2: cleanup unconditionally.
    let _ = client.delete_underlying(&symbol).await;
    let after = client.get_global_stats().await;

    // Phase 3: assert.
    let before = before.expect("stats before");
    created.expect("create underlying");
    let during = during.expect("stats during");
    let after = after.expect("stats after");
    assert_eq!(
        during.underlying_count,
        before.underlying_count + 1,
        "creating an underlying must raise the count by one"
    );
    assert_eq!(
        after.underlying_count, before.underlying_count,
        "deleting it must restore the count"
    );
}

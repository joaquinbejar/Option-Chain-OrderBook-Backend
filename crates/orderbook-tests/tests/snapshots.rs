//! Admin snapshot retention tests (issue #58).
//!
//! Verifies that snapshot storage is bounded: creating snapshots past the
//! server's retention cap (16) evicts the oldest ones, and the create
//! response reports serialization failures separately from saved books.

use orderbook_client::Permission;
use orderbook_tests::create_authenticated_client;

/// Mirrors `AppState::MAX_RETAINED_SNAPSHOTS` on the server.
const MAX_RETAINED_SNAPSHOTS: usize = 16;

#[tokio::test]
async fn test_snapshots_are_bounded_and_oldest_evicted() {
    let client = create_authenticated_client(vec![Permission::Admin])
        .await
        .expect("Failed to create admin client");

    // Create enough snapshots to exceed the retention cap.
    let mut created_ids: Vec<String> = Vec::new();
    for _ in 0..(MAX_RETAINED_SNAPSHOTS + 2) {
        let response = client
            .create_snapshot()
            .await
            .expect("Failed to create snapshot");
        assert!(response.success);
        assert_eq!(
            response.orderbooks_failed, 0,
            "no orderbook should fail to serialize on a healthy server"
        );
        created_ids.push(response.snapshot_id);
    }

    let list = client
        .list_snapshots()
        .await
        .expect("Failed to list snapshots");
    assert!(
        list.total as usize <= MAX_RETAINED_SNAPSHOTS,
        "retained snapshots ({}) must not exceed the cap ({})",
        list.total,
        MAX_RETAINED_SNAPSHOTS
    );

    // The newest snapshot survives; the first one we created was evicted
    // (we alone created cap + 2, so at least our two oldest are gone).
    let newest = created_ids.last().expect("created at least one snapshot");
    let oldest = created_ids.first().expect("created at least one snapshot");
    let retained: Vec<String> = list
        .snapshots
        .iter()
        .map(|s| s.snapshot_id.clone())
        .collect();
    assert!(
        retained.contains(newest),
        "newest snapshot must still be retained"
    );
    assert!(
        !retained.contains(oldest),
        "oldest snapshot past the cap must have been evicted"
    );

    // The evicted snapshot is also gone from the by-ID endpoint (404).
    let err = client.get_snapshot(oldest).await;
    assert!(err.is_err(), "evicted snapshot must return an error (404)");
}

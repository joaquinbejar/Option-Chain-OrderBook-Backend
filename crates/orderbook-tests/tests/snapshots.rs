//! Admin snapshot retention tests (issue #58).
//!
//! Verifies that snapshot storage is bounded: creating snapshots past the
//! server's retention cap (16) evicts the oldest ones, and the create
//! response reports serialization failures separately from saved books.

use orderbook_client::Error;
use orderbook_tests::admin_client;

/// Mirrors `AppState::MAX_RETAINED_SNAPSHOTS` on the server.
const MAX_RETAINED_SNAPSHOTS: usize = 16;

#[tokio::test]
async fn test_snapshots_are_bounded_and_oldest_evicted() {
    let client = admin_client().await.expect("Failed to create admin client");

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
    match client.get_snapshot(oldest).await {
        Err(Error::NotFound(_)) => {}
        other => panic!("evicted snapshot must return NotFound (404), got {other:?}"),
    }
}

/// Issue #136 / #110: the create → restore round-trip completes without a
/// server panic, restoring every saved book (the legacy shadow-book snapshot
/// symbols that used to hit an upstream chrono-overflow panic can no longer
/// be produced, and absurd day counts are rejected at parse time).
#[tokio::test]
async fn test_snapshot_create_restore_round_trip() {
    let client = admin_client().await.expect("admin client");

    let created = client.create_snapshot().await.expect("create snapshot");
    assert!(created.success);
    assert_eq!(created.orderbooks_failed, 0);

    let restored = client
        .restore_snapshot(&created.snapshot_id)
        .await
        .expect("restore snapshot must not drop the connection");
    assert!(restored.success, "no book may fail to restore");
    assert_eq!(restored.orderbooks_failed, 0);
    assert_eq!(
        restored.orderbooks_restored, created.orderbooks_saved,
        "every saved book restores"
    );
    assert_eq!(restored.orders_restored, created.orders_saved);
}

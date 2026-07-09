//! Market maker control and price endpoint tests.
//!
//! The control endpoints (`/api/v1/controls/*`) require an `Admin` token and act
//! on GLOBAL market-maker state, so every test that mutates a global parameter
//! (a) serializes with the shared [`control_lock`] and (b) uses the
//! capture-then-assert pattern — perform every request first, capturing outcomes
//! into plain variables, then run the restore/cleanup, and only THEN assert — so
//! a failed assertion never leaves the server mutated for the next test.

use orderbook_client::{InsertPriceRequest, UpdateParametersRequest};
use orderbook_tests::{admin_client, cleanup_underlying, control_lock, read_client, unique_symbol};

#[tokio::test]
async fn test_get_controls() {
    // Read under the control lock so we never observe another test's transient
    // mid-mutation state.
    let _guard = control_lock().lock().await;
    let client = admin_client().await.expect("admin client");

    let controls = client.get_controls().await.expect("Failed to get controls");

    // Controls should have valid values.
    assert!(controls.spread_multiplier > 0.0);
    assert!(controls.size_scalar >= 0.0 && controls.size_scalar <= 1.0);
    assert!(controls.directional_skew >= -1.0 && controls.directional_skew <= 1.0);
}

#[tokio::test]
async fn test_kill_switch_and_enable() {
    let _guard = control_lock().lock().await;
    let client = admin_client().await.expect("admin client");

    // Record the starting state so we can restore it.
    let initial = client.get_controls().await.expect("controls");

    // Phase 1: perform every mutation, capturing outcomes (no asserts interleaved).
    let kill_response = client.kill_switch().await;
    let after_kill = client.get_controls().await;
    let enable_response = client.enable_quoting().await;
    let after_enable = client.get_controls().await;

    // Phase 2: restore the initial master state BEFORE asserting. Both endpoints
    // set an absolute state, so calling the correct one unconditionally restores
    // `initial` even if a mutation above failed part-way.
    let _restore = if initial.master_enabled {
        client.enable_quoting().await
    } else {
        client.kill_switch().await
    };

    // Phase 3: assert on the captured values.
    let kill_response = kill_response.expect("Failed to activate kill switch");
    assert!(kill_response.success);
    assert!(!kill_response.master_enabled);

    let after_kill = after_kill.expect("Failed to get controls after kill");
    assert!(!after_kill.master_enabled);

    let enable_response = enable_response.expect("Failed to enable quoting");
    assert!(enable_response.success);
    assert!(enable_response.master_enabled);

    let after_enable = after_enable.expect("Failed to get controls after enable");
    assert!(after_enable.master_enabled);
}

#[tokio::test]
async fn test_update_parameters() {
    let _guard = control_lock().lock().await;
    let client = admin_client().await.expect("admin client");

    let initial = client.get_controls().await.expect("Failed to get controls");

    // Phase 1: mutate and read back, capturing outcomes.
    let update_response = client
        .update_parameters(&UpdateParametersRequest {
            spread_multiplier: Some(1.5),
            size_scalar: Some(0.75), // fraction of base size (issue #82)
            directional_skew: Some(0.1),
        })
        .await;
    let read_back = client.get_controls().await;

    // Phase 2: restore the original parameters (absolute set, same representation)
    // BEFORE asserting, so a failed assertion never leaks a mutated config.
    let _restore = client
        .update_parameters(&UpdateParametersRequest {
            spread_multiplier: Some(initial.spread_multiplier),
            size_scalar: Some(initial.size_scalar),
            directional_skew: Some(initial.directional_skew),
        })
        .await;

    // Phase 3: assert on the captured values.
    let update_response = update_response.expect("Failed to update parameters");
    assert!(update_response.success);
    assert!((update_response.spread_multiplier - 1.5).abs() < 0.01);
    assert!((update_response.size_scalar - 0.75).abs() < 0.01);
    assert!((update_response.directional_skew - 0.1).abs() < 0.01);

    // Issue #82 acceptance: the value GET reports round-trips through a POST.
    let read_back = read_back.expect("Failed to get controls");
    assert!((read_back.size_scalar - 0.75).abs() < 0.01);
}

#[tokio::test]
async fn test_list_instruments() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("INST");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // Capture the read, then clean up, then assert.
    let instruments = client.list_instruments().await;

    cleanup_underlying(&client, &symbol).await;

    let instruments = instruments.expect("Failed to list instruments");
    let found = instruments.instruments.iter().any(|i| i.symbol == symbol);
    assert!(found, "Created instrument not found in list");
}

#[tokio::test]
async fn test_toggle_instrument() {
    // Per-instrument quoting is part of the market-maker controls surface;
    // serialize it with the other control tests for good measure.
    let _guard = control_lock().lock().await;
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("TOG");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // Phase 1: two sequential toggles, captured.
    let toggle1 = client.toggle_instrument(&symbol).await;
    let toggle2 = client.toggle_instrument(&symbol).await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &symbol).await;

    // Phase 3: assert.
    let toggle1 = toggle1.expect("Failed to toggle instrument");
    assert!(toggle1.success);
    assert_eq!(toggle1.symbol, symbol);
    let first_state = toggle1.enabled;

    let toggle2 = toggle2.expect("Failed to toggle instrument again");
    assert!(toggle2.success);
    assert_eq!(toggle2.enabled, !first_state);
}

#[tokio::test]
async fn test_insert_and_get_price() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("PRC");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // Phase 1: insert then read the price back, captured.
    let insert_response = client
        .insert_price(&InsertPriceRequest {
            symbol: symbol.clone(),
            price: 100.50,
            bid: Some(100.25),
            ask: Some(100.75),
            volume: Some(1000),
            source: Some("test".to_string()),
        })
        .await;
    let price = client.get_latest_price(&symbol).await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &symbol).await;

    // Phase 3: assert.
    let insert_response = insert_response.expect("Failed to insert price");
    assert!(insert_response.success);
    assert_eq!(insert_response.symbol, symbol);
    assert_eq!(insert_response.price_cents, 10050);

    let price = price.expect("Failed to get price");
    assert_eq!(price.symbol, symbol);
    assert!((price.price - 100.50).abs() < 0.01);
}

#[tokio::test]
async fn test_get_all_prices() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("ALL");

    client.create_underlying(&symbol).await.expect("create");

    // Phase 1: insert a price and read the full list, captured.
    let insert = client
        .insert_price(&InsertPriceRequest {
            symbol: symbol.clone(),
            price: 50.0,
            bid: None,
            ask: None,
            volume: None,
            source: None,
        })
        .await;
    let prices = client.get_all_prices().await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &symbol).await;

    // Phase 3: assert.
    insert.expect("insert price");
    let prices = prices.expect("Failed to get all prices");
    let found = prices.iter().any(|p| p.symbol == symbol);
    assert!(found, "Inserted price not found in all prices");
}

#[tokio::test]
async fn test_price_not_found() {
    let client = read_client().await.expect("read client");

    // A symbol that was never priced returns 404 -> NotFound. Read-only: no
    // underlying is created and no control state is touched, so no cleanup.
    let result = client.get_latest_price("NONEXISTENT_SYMBOL_12345").await;
    assert!(result.is_err());
}

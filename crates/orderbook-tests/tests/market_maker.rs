//! Market maker control and price endpoint tests.
//!
//! The control endpoints (`/api/v1/controls/*`) require an `Admin` token and act
//! on GLOBAL market-maker state, so every test that mutates a global parameter
//! restores it before returning.

use orderbook_client::{InsertPriceRequest, UpdateParametersRequest};
use orderbook_tests::{admin_client, cleanup_underlying, read_client, unique_symbol};

#[tokio::test]
async fn test_get_controls() {
    let client = admin_client().await.expect("admin client");

    let controls = client.get_controls().await.expect("Failed to get controls");

    // Controls should have valid values.
    assert!(controls.spread_multiplier > 0.0);
    assert!(controls.size_scalar >= 0.0 && controls.size_scalar <= 1.0);
    assert!(controls.directional_skew >= -1.0 && controls.directional_skew <= 1.0);
}

#[tokio::test]
async fn test_kill_switch_and_enable() {
    let client = admin_client().await.expect("admin client");

    // Record the starting state so we can restore it.
    let initial = client.get_controls().await.expect("controls");

    // Activate kill switch.
    let kill_response = client
        .kill_switch()
        .await
        .expect("Failed to activate kill switch");
    assert!(kill_response.success);
    assert!(!kill_response.master_enabled);

    let controls = client.get_controls().await.expect("Failed to get controls");
    assert!(!controls.master_enabled);

    // Re-enable quoting.
    let enable_response = client
        .enable_quoting()
        .await
        .expect("Failed to enable quoting");
    assert!(enable_response.success);
    assert!(enable_response.master_enabled);

    let controls = client.get_controls().await.expect("Failed to get controls");
    assert!(controls.master_enabled);

    // Restore the initial master state.
    if !initial.master_enabled {
        client.kill_switch().await.expect("restore kill state");
    }
}

#[tokio::test]
async fn test_update_parameters() {
    let client = admin_client().await.expect("admin client");

    let initial = client.get_controls().await.expect("Failed to get controls");

    let update_response = client
        .update_parameters(&UpdateParametersRequest {
            spread_multiplier: Some(1.5),
            size_scalar: Some(0.75), // fraction of base size (issue #82)
            directional_skew: Some(0.1),
        })
        .await
        .expect("Failed to update parameters");

    assert!(update_response.success);
    assert!((update_response.spread_multiplier - 1.5).abs() < 0.01);
    assert!((update_response.size_scalar - 0.75).abs() < 0.01);
    assert!((update_response.directional_skew - 0.1).abs() < 0.01);

    // Issue #82 acceptance: the value GET reports round-trips through a POST.
    let read_back = client.get_controls().await.expect("Failed to get controls");
    assert!((read_back.size_scalar - 0.75).abs() < 0.01);

    // Restore the original parameters — the same representation, no conversion.
    client
        .update_parameters(&UpdateParametersRequest {
            spread_multiplier: Some(initial.spread_multiplier),
            size_scalar: Some(initial.size_scalar),
            directional_skew: Some(initial.directional_skew),
        })
        .await
        .expect("Failed to restore parameters");
}

#[tokio::test]
async fn test_list_instruments() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("INST");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    let instruments = client
        .list_instruments()
        .await
        .expect("Failed to list instruments");

    let found = instruments.instruments.iter().any(|i| i.symbol == symbol);
    assert!(found, "Created instrument not found in list");

    cleanup_underlying(&client, &symbol).await;
}

#[tokio::test]
async fn test_toggle_instrument() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("TOG");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    let toggle1 = client
        .toggle_instrument(&symbol)
        .await
        .expect("Failed to toggle instrument");
    assert!(toggle1.success);
    assert_eq!(toggle1.symbol, symbol);
    let first_state = toggle1.enabled;

    let toggle2 = client
        .toggle_instrument(&symbol)
        .await
        .expect("Failed to toggle instrument again");
    assert!(toggle2.success);
    assert_eq!(toggle2.enabled, !first_state);

    cleanup_underlying(&client, &symbol).await;
}

#[tokio::test]
async fn test_insert_and_get_price() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("PRC");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    let insert_response = client
        .insert_price(&InsertPriceRequest {
            symbol: symbol.clone(),
            price: 100.50,
            bid: Some(100.25),
            ask: Some(100.75),
            volume: Some(1000),
            source: Some("test".to_string()),
        })
        .await
        .expect("Failed to insert price");

    assert!(insert_response.success);
    assert_eq!(insert_response.symbol, symbol);
    assert_eq!(insert_response.price_cents, 10050);

    let price = client
        .get_latest_price(&symbol)
        .await
        .expect("Failed to get price");

    assert_eq!(price.symbol, symbol);
    assert!((price.price - 100.50).abs() < 0.01);

    cleanup_underlying(&client, &symbol).await;
}

#[tokio::test]
async fn test_get_all_prices() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("ALL");

    client.create_underlying(&symbol).await.expect("create");
    client
        .insert_price(&InsertPriceRequest {
            symbol: symbol.clone(),
            price: 50.0,
            bid: None,
            ask: None,
            volume: None,
            source: None,
        })
        .await
        .expect("insert price");

    let prices = client
        .get_all_prices()
        .await
        .expect("Failed to get all prices");

    let found = prices.iter().any(|p| p.symbol == symbol);
    assert!(found, "Inserted price not found in all prices");

    cleanup_underlying(&client, &symbol).await;
}

#[tokio::test]
async fn test_price_not_found() {
    let client = read_client().await.expect("read client");

    // A symbol that was never priced returns 404 -> NotFound.
    let result = client.get_latest_price("NONEXISTENT_SYMBOL_12345").await;
    assert!(result.is_err());
}

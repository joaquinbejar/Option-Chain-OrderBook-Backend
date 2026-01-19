//! Market maker control endpoint tests.

use orderbook_client::{InsertPriceRequest, UpdateParametersRequest};
use orderbook_tests::{create_test_client, unique_symbol};

#[tokio::test]
async fn test_get_controls() {
    let client = create_test_client().expect("Failed to create client");

    let controls = client.get_controls().await.expect("Failed to get controls");

    // Controls should have valid values
    assert!(controls.spread_multiplier > 0.0);
    assert!(controls.size_scalar >= 0.0);
    assert!(controls.directional_skew >= -1.0 && controls.directional_skew <= 1.0);
}

#[tokio::test]
async fn test_kill_switch_and_enable() {
    let client = create_test_client().expect("Failed to create client");

    // Activate kill switch
    let kill_response = client
        .kill_switch()
        .await
        .expect("Failed to activate kill switch");
    assert!(kill_response.success);
    assert!(!kill_response.master_enabled);

    // Verify controls show disabled
    let controls = client.get_controls().await.expect("Failed to get controls");
    assert!(!controls.master_enabled);

    // Re-enable quoting
    let enable_response = client
        .enable_quoting()
        .await
        .expect("Failed to enable quoting");
    assert!(enable_response.success);
    assert!(enable_response.master_enabled);

    // Verify controls show enabled
    let controls = client.get_controls().await.expect("Failed to get controls");
    assert!(controls.master_enabled);
}

#[tokio::test]
async fn test_update_parameters() {
    let client = create_test_client().expect("Failed to create client");

    // Get current parameters
    let initial = client.get_controls().await.expect("Failed to get controls");

    // Update parameters
    let update_response = client
        .update_parameters(&UpdateParametersRequest {
            spread_multiplier: Some(1.5),
            size_scalar: Some(75.0), // 75%
            directional_skew: Some(0.1),
        })
        .await
        .expect("Failed to update parameters");

    assert!(update_response.success);
    assert!((update_response.spread_multiplier - 1.5).abs() < 0.01);
    assert!((update_response.size_scalar - 75.0).abs() < 0.01);
    assert!((update_response.directional_skew - 0.1).abs() < 0.01);

    // Restore original parameters
    client
        .update_parameters(&UpdateParametersRequest {
            spread_multiplier: Some(initial.spread_multiplier),
            size_scalar: Some(initial.size_scalar * 100.0), // Convert back to percentage
            directional_skew: Some(initial.directional_skew),
        })
        .await
        .expect("Failed to restore parameters");
}

#[tokio::test]
async fn test_list_instruments() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("INST");

    // Create an underlying first
    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // List instruments
    let instruments = client
        .list_instruments()
        .await
        .expect("Failed to list instruments");

    // Should include our new instrument
    let found = instruments.instruments.iter().any(|i| i.symbol == symbol);
    assert!(found, "Created instrument not found in list");

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_toggle_instrument() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("TOG");

    // Create an underlying
    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // Toggle instrument (should disable if enabled, or enable if disabled)
    let toggle1 = client
        .toggle_instrument(&symbol)
        .await
        .expect("Failed to toggle instrument");
    assert!(toggle1.success);
    assert_eq!(toggle1.symbol, symbol);
    let first_state = toggle1.enabled;

    // Toggle again (should reverse)
    let toggle2 = client
        .toggle_instrument(&symbol)
        .await
        .expect("Failed to toggle instrument again");
    assert!(toggle2.success);
    assert_eq!(toggle2.enabled, !first_state);

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_insert_and_get_price() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("PRC");

    // Create underlying first
    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // Insert a price
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

    // Get the price
    let price = client
        .get_latest_price(&symbol)
        .await
        .expect("Failed to get price");

    assert_eq!(price.symbol, symbol);
    assert!((price.price - 100.50).abs() < 0.01);

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_get_all_prices() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("ALL");

    // Create underlying and insert price
    client.create_underlying(&symbol).await.unwrap();
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
        .unwrap();

    // Get all prices
    let prices = client
        .get_all_prices()
        .await
        .expect("Failed to get all prices");

    // Should include our price
    let found = prices.iter().any(|p| p.symbol == symbol);
    assert!(found, "Inserted price not found in all prices");

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_price_not_found() {
    let client = create_test_client().expect("Failed to create client");

    // Try to get price for non-existent symbol
    let result = client.get_latest_price("NONEXISTENT_SYMBOL_12345").await;

    // Should return NotFound error
    assert!(result.is_err());
}

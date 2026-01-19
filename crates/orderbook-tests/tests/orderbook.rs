//! Orderbook CRUD operation tests.

use orderbook_client::{
    AddOrderRequest, MarketOrderRequest, MarketOrderStatus, OptionPath, OrderSide,
};
use orderbook_tests::{create_test_client, unique_symbol};

#[tokio::test]
async fn test_create_and_list_underlyings() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("TEST");

    // Create underlying
    let created = client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");
    assert_eq!(created.symbol, symbol);

    // List underlyings should include our new one
    let list = client
        .list_underlyings()
        .await
        .expect("Failed to list underlyings");
    assert!(list.underlyings.contains(&symbol));

    // Get underlying
    let fetched = client
        .get_underlying(&symbol)
        .await
        .expect("Failed to get underlying");
    assert_eq!(fetched.symbol, symbol);

    // Clean up
    client
        .delete_underlying(&symbol)
        .await
        .expect("Failed to delete underlying");
}

#[tokio::test]
async fn test_create_expiration_and_strike() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("EXP");
    let expiration = "20251231";
    let strike = 10000u64;

    // Create underlying
    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // Create expiration
    let exp = client
        .create_expiration(&symbol, expiration)
        .await
        .expect("Failed to create expiration");
    assert_eq!(exp.expiration, expiration);

    // List expirations
    let exps = client
        .list_expirations(&symbol)
        .await
        .expect("Failed to list expirations");
    assert!(exps.expirations.contains(&expiration.to_string()));

    // Create strike
    let strike_info = client
        .create_strike(&symbol, expiration, strike)
        .await
        .expect("Failed to create strike");
    assert_eq!(strike_info.strike, strike);

    // List strikes
    let strikes = client
        .list_strikes(&symbol, expiration)
        .await
        .expect("Failed to list strikes");
    assert!(strikes.strikes.contains(&strike));

    // Clean up
    client
        .delete_underlying(&symbol)
        .await
        .expect("Failed to delete underlying");
}

#[tokio::test]
async fn test_add_and_cancel_order() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("ORD");
    let expiration = "20251231";
    let strike = 10000u64;

    // Setup: create underlying, expiration, strike
    client.create_underlying(&symbol).await.unwrap();
    client.create_expiration(&symbol, expiration).await.unwrap();
    client
        .create_strike(&symbol, expiration, strike)
        .await
        .unwrap();

    let option = OptionPath::call(&symbol, expiration, strike);

    // Add a limit order
    let order = client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1500,
                quantity: 10,
            },
        )
        .await
        .expect("Failed to add order");

    assert!(!order.order_id.is_empty());
    assert!(order.message.contains("success"));

    // Get order book should show the order
    let book = client
        .get_option_book(&option)
        .await
        .expect("Failed to get order book");
    assert!(book.order_count > 0);

    // Cancel the order
    let cancel = client
        .cancel_order(&option, &order.order_id)
        .await
        .expect("Failed to cancel order");
    assert!(cancel.success);

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_get_option_quote() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("QUO");
    let expiration = "20251231";
    let strike = 10000u64;

    // Setup
    client.create_underlying(&symbol).await.unwrap();
    client.create_expiration(&symbol, expiration).await.unwrap();
    client
        .create_strike(&symbol, expiration, strike)
        .await
        .unwrap();

    let option = OptionPath::call(&symbol, expiration, strike);

    // Add bid and ask orders
    client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 10,
            },
        )
        .await
        .unwrap();

    client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1600,
                quantity: 10,
            },
        )
        .await
        .unwrap();

    // Get quote
    let quote = client
        .get_option_quote(&option)
        .await
        .expect("Failed to get quote");

    assert_eq!(quote.bid_price, Some(1400));
    assert_eq!(quote.ask_price, Some(1600));
    assert_eq!(quote.bid_size, 10);
    assert_eq!(quote.ask_size, 10);

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_market_order_execution() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("MKT");
    let expiration = "20251231";
    let strike = 10000u64;

    // Setup
    client.create_underlying(&symbol).await.unwrap();
    client.create_expiration(&symbol, expiration).await.unwrap();
    client
        .create_strike(&symbol, expiration, strike)
        .await
        .unwrap();

    let option = OptionPath::call(&symbol, expiration, strike);

    // Add a sell order (liquidity)
    client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 100,
            },
        )
        .await
        .unwrap();

    // Submit market buy order
    let result = client
        .submit_market_order(
            &option,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 50,
            },
        )
        .await
        .expect("Failed to submit market order");

    assert_eq!(result.status, MarketOrderStatus::Filled);
    assert_eq!(result.filled_quantity, 50);
    assert_eq!(result.remaining_quantity, 0);
    assert!(result.average_price.is_some());
    assert!(!result.fills.is_empty());

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_market_order_no_liquidity() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("NLQ");
    let expiration = "20251231";
    let strike = 10000u64;

    // Setup (no orders in the book)
    client.create_underlying(&symbol).await.unwrap();
    client.create_expiration(&symbol, expiration).await.unwrap();
    client
        .create_strike(&symbol, expiration, strike)
        .await
        .unwrap();

    let option = OptionPath::call(&symbol, expiration, strike);

    // Submit market order with no liquidity - should fail
    let result = client
        .submit_market_order(
            &option,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 50,
            },
        )
        .await;

    // Should return an error due to no liquidity
    assert!(result.is_err());

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

#[tokio::test]
async fn test_put_option_operations() {
    let client = create_test_client().expect("Failed to create client");
    let symbol = unique_symbol("PUT");
    let expiration = "20251231";
    let strike = 10000u64;

    // Setup
    client.create_underlying(&symbol).await.unwrap();
    client.create_expiration(&symbol, expiration).await.unwrap();
    client
        .create_strike(&symbol, expiration, strike)
        .await
        .unwrap();

    let option = OptionPath::put(&symbol, expiration, strike);

    // Add order to put option
    let order = client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 500,
                quantity: 20,
            },
        )
        .await
        .expect("Failed to add put order");

    assert!(!order.order_id.is_empty());

    // Get put order book
    let book = client
        .get_option_book(&option)
        .await
        .expect("Failed to get put order book");
    assert!(book.order_count > 0);

    // Clean up
    client.delete_underlying(&symbol).await.unwrap();
}

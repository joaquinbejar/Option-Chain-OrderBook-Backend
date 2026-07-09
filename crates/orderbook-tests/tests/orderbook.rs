//! Orderbook CRUD operation tests.
//!
//! Placement uses [`TEST_EXPIRATION`]; reads/cancels use the server-formatted
//! expiration returned by the setup helpers (see the crate docs for bug #110).

use orderbook_client::{
    AddOrderRequest, MarketOrderRequest, MarketOrderStatus, OptionPath, OrderSide,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, formatted_expiration,
    setup_underlying, unique_symbol,
};

#[tokio::test]
async fn test_create_and_list_underlyings() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("TEST");

    let created = client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");
    assert_eq!(created.symbol, symbol);

    let list = client
        .list_underlyings()
        .await
        .expect("Failed to list underlyings");
    assert!(list.underlyings.contains(&symbol));

    let fetched = client
        .get_underlying(&symbol)
        .await
        .expect("Failed to get underlying");
    assert_eq!(fetched.symbol, symbol);

    // The delete returns a typed confirmation (issue #60).
    let deleted = client
        .delete_underlying(&symbol)
        .await
        .expect("Failed to delete underlying");
    assert!(deleted.success);
    assert!(deleted.message.contains(&symbol));
}

#[tokio::test]
async fn test_create_expiration_and_strike() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("EXP");

    client
        .create_underlying(&symbol)
        .await
        .expect("Failed to create underlying");

    // create_expiration returns the server's canonical expiration form (bug #110
    // formats the parsed Days value), so assert only that it succeeds.
    let exp = client
        .create_expiration(&symbol, TEST_EXPIRATION)
        .await
        .expect("Failed to create expiration");
    assert!(!exp.expiration.is_empty());

    // list_expirations reports the URL-safe formatted expiration used for reads.
    let formatted = formatted_expiration(&client, &symbol).await;
    let exps = client
        .list_expirations(&symbol)
        .await
        .expect("Failed to list expirations");
    assert!(exps.expirations.contains(&formatted));

    let strike_info = client
        .create_strike(&symbol, TEST_EXPIRATION, TEST_STRIKE)
        .await
        .expect("Failed to create strike");
    assert_eq!(strike_info.strike, TEST_STRIKE);

    // list_strikes resolves the book by the formatted expiration.
    let strikes = client
        .list_strikes(&symbol, &formatted)
        .await
        .expect("Failed to list strikes");
    assert!(strikes.strikes.contains(&TEST_STRIKE));

    cleanup_underlying(&client, &symbol).await;
}

#[tokio::test]
async fn test_add_and_cancel_order() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "ORD").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);

    let order = client
        .add_order(
            &place,
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

    let book = client
        .get_option_book(&read)
        .await
        .expect("Failed to get order book");
    assert!(book.order_count > 0);

    let cancel = client
        .cancel_order(&read, &order.order_id)
        .await
        .expect("Failed to cancel order");
    assert!(cancel.success);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_get_option_quote() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "QUO").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);

    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 10,
            },
        )
        .await
        .expect("add bid");

    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1600,
                quantity: 10,
            },
        )
        .await
        .expect("add ask");

    let quote = client
        .get_option_quote(&read)
        .await
        .expect("Failed to get quote");

    assert_eq!(quote.bid_price, Some(1400));
    assert_eq!(quote.ask_price, Some(1600));
    assert_eq!(quote.bid_size, 10);
    assert_eq!(quote.ask_size, 10);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_market_order_execution() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "MKT").await;

    // Placement resolves the book by parsing TEST_EXPIRATION, so the resting sell
    // and the market buy land on the same book and cross.
    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 100,
            },
        )
        .await
        .expect("add resting sell");

    let result = client
        .submit_market_order(
            &place,
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

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_market_order_no_liquidity() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "NLQ").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    // No resting liquidity: the market order is rejected with an error.
    let result = client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 50,
            },
        )
        .await;

    assert!(result.is_err());

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_put_option_operations() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "PUT").await;

    let place = OptionPath::put(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let read = OptionPath::put(&underlying, &formatted, TEST_STRIKE);

    let order = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 500,
                quantity: 20,
            },
        )
        .await
        .expect("Failed to add put order");
    assert!(!order.order_id.is_empty());

    let book = client
        .get_option_book(&read)
        .await
        .expect("Failed to get put order book");
    assert!(book.order_count > 0);

    cleanup_underlying(&client, &underlying).await;
}

//! Orderbook CRUD operation tests.
//!
//! Placement uses [`TEST_EXPIRATION`]; reads/cancels use the server-formatted
//! expiration returned by the setup helpers (see the crate docs for bug #110).
//!
//! Every test that creates an underlying uses the capture-then-assert pattern:
//! perform all requests capturing outcomes into plain variables, run
//! [`cleanup_underlying`], and only THEN assert — so a failed assertion never
//! leaks a test underlying on the server.

use orderbook_client::{
    AddOrderRequest, MarketOrderRequest, MarketOrderStatus, OptionPath, OrderSide,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, setup_underlying, unique_symbol,
};

#[tokio::test]
async fn test_create_and_list_underlyings() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("TEST");

    // Phase 1: create / list / get / delete, captured. The delete is both the
    // final assertion target AND the cleanup, so it always runs.
    let created = client.create_underlying(&symbol).await;
    let list = client.list_underlyings().await;
    let fetched = client.get_underlying(&symbol).await;
    let deleted = client.delete_underlying(&symbol).await;

    // Phase 2: assert on the captured values.
    let created = created.expect("Failed to create underlying");
    assert_eq!(created.symbol, symbol);

    let list = list.expect("Failed to list underlyings");
    assert!(list.underlyings.contains(&symbol));

    let fetched = fetched.expect("Failed to get underlying");
    assert_eq!(fetched.symbol, symbol);

    // The delete returns a typed confirmation (issue #60).
    let deleted = deleted.expect("Failed to delete underlying");
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

    // Phase 1: create expiration, list them, create strike, list strikes.
    // create_expiration returns the server's canonical expiration form (bug #110
    // formats the parsed Days value). list_expirations reports the URL-safe
    // formatted expiration used for reads; list_strikes resolves the book by it.
    let exp = client.create_expiration(&symbol, TEST_EXPIRATION).await;
    let exps = client.list_expirations(&symbol).await;
    let strike_info = client
        .create_strike(&symbol, TEST_EXPIRATION, TEST_STRIKE)
        .await;
    let formatted = exps
        .as_ref()
        .ok()
        .and_then(|e| e.expirations.first().cloned());
    let strikes = match &formatted {
        Some(f) => Some(client.list_strikes(&symbol, f).await),
        None => None,
    };

    // Phase 2: cleanup.
    cleanup_underlying(&client, &symbol).await;

    // Phase 3: assert.
    let exp = exp.expect("Failed to create expiration");
    assert!(!exp.expiration.is_empty());

    let exps = exps.expect("Failed to list expirations");
    let formatted = formatted.expect("underlying has at least one expiration");
    assert!(exps.expirations.contains(&formatted));

    let strike_info = strike_info.expect("Failed to create strike");
    assert_eq!(strike_info.strike, TEST_STRIKE);

    let strikes = strikes
        .expect("list_strikes should have run")
        .expect("Failed to list strikes");
    assert!(strikes.strikes.contains(&TEST_STRIKE));
}

#[tokio::test]
async fn test_add_and_cancel_order() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "ORD").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);

    // Phase 1: add, read the book, cancel (the cancel depends on the order id).
    let order = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1500,
                quantity: 10,
            },
        )
        .await;
    let book = client.get_option_book(&read).await;
    let cancel = match order.as_ref().ok() {
        Some(o) => Some(client.cancel_order(&read, &o.order_id).await),
        None => None,
    };

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    let order = order.expect("Failed to add order");
    assert!(!order.order_id.is_empty());
    assert!(order.message.contains("success"));

    let book = book.expect("Failed to get order book");
    assert!(book.order_count > 0);

    let cancel = cancel
        .expect("order should have succeeded")
        .expect("Failed to cancel order");
    assert!(cancel.success);
}

#[tokio::test]
async fn test_get_option_quote() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "QUO").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);

    // Phase 1: rest a bid and an ask, then read the quote.
    let bid = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 10,
            },
        )
        .await;
    let ask = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1600,
                quantity: 10,
            },
        )
        .await;
    let quote = client.get_option_quote(&read).await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    bid.expect("add bid");
    ask.expect("add ask");
    let quote = quote.expect("Failed to get quote");
    assert_eq!(quote.bid_price, Some(1400));
    assert_eq!(quote.ask_price, Some(1600));
    assert_eq!(quote.bid_size, 10);
    assert_eq!(quote.ask_size, 10);
}

#[tokio::test]
async fn test_market_order_execution() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "MKT").await;

    // Placement resolves the book by parsing TEST_EXPIRATION, so the resting sell
    // and the market buy land on the same book and cross.
    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    // Phase 1: rest a sell, then take it with a market buy.
    let rest = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 100,
            },
        )
        .await;
    let result = client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 50,
            },
        )
        .await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    rest.expect("add resting sell");
    let result = result.expect("Failed to submit market order");
    assert_eq!(result.status, MarketOrderStatus::Filled);
    assert_eq!(result.filled_quantity, 50);
    assert_eq!(result.remaining_quantity, 0);
    // Issue #87: assert the exact cents values, not mere presence — a fill at
    // the wrong price must fail this test. One resting sell at 1500 means
    // every fill and the average are exactly 1500.
    assert_eq!(result.average_price, Some(1500.0));
    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].price, 1500);
    assert_eq!(result.fills[0].quantity, 50);
}

/// Issue #87: a market order sweeping two price levels must report each fill
/// at its level price and the exact quantity-weighted average.
#[tokio::test]
async fn test_market_order_weighted_average_across_levels() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "AVG").await;
    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    // Phase 1: rest two sell levels, then take across both.
    let rest_low = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1400,
                quantity: 30,
            },
        )
        .await;
    let rest_high = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 30,
            },
        )
        .await;
    let result = client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 50,
            },
        )
        .await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    rest_low.expect("add 30 @ 1400");
    rest_high.expect("add 30 @ 1500");
    let result = result.expect("Failed to submit market order");
    assert_eq!(result.status, MarketOrderStatus::Filled);
    assert_eq!(result.filled_quantity, 50);
    assert_eq!(result.remaining_quantity, 0);

    // 30 @ 1400 + 20 @ 1500 => (30*1400 + 20*1500) / 50 = 1440 exactly.
    assert_eq!(result.average_price, Some(1440.0));
    let mut fills: Vec<(u128, u64)> = result.fills.iter().map(|f| (f.price, f.quantity)).collect();
    fills.sort_unstable();
    assert_eq!(fills, vec![(1400, 30), (1500, 20)]);
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

    cleanup_underlying(&client, &underlying).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_put_option_operations() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "PUT").await;

    let place = OptionPath::put(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let read = OptionPath::put(&underlying, &formatted, TEST_STRIKE);

    // Phase 1: add a put order, then read the put book.
    let order = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 500,
                quantity: 20,
            },
        )
        .await;
    let book = client.get_option_book(&read).await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    let order = order.expect("Failed to add put order");
    assert!(!order.order_id.is_empty());

    let book = book.expect("Failed to get put order book");
    assert!(book.order_count > 0);
}

//! Position P&L and execution-report tests driven by the caller's own crossing
//! fills (bug #110 makes filling against market-maker quotes impossible, so all
//! liquidity here is self-provided).
//!
//! Both tests create an underlying, so each uses the capture-then-assert pattern:
//! all requests run first, then [`cleanup_underlying`], and only THEN the
//! assertions — a failed assertion never leaks a test underlying.

use orderbook_client::{
    AddOrderRequest, ExecutionsQuery, MarketOrderRequest, MarketOrderStatus, OptionPath, OrderSide,
    PositionQuery,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, setup_underlying,
};

#[tokio::test]
async fn test_priced_position_pnl() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "FPN").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    // Phase 1: rest an ask at 1600 and a bid at 1400, then take 40 from the ask.
    // The resulting long (40 @ 1600) is marked at the 1500 mid, for a -4000 PnL.
    let ask = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1600,
                quantity: 100,
            },
        )
        .await;
    let bid = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 50,
            },
        )
        .await;
    let fill = client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 40,
            },
        )
        .await;

    let symbol = format!("{underlying}-{TEST_EXPIRATION}-{TEST_STRIKE}-C");
    let position = client.get_position(&symbol).await;
    let listed = client
        .list_positions(Some(&PositionQuery {
            underlying: Some(underlying.clone()),
        }))
        .await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    ask.expect("rest ask");
    bid.expect("rest bid");
    let fill = fill.expect("market buy");
    assert_eq!(fill.status, MarketOrderStatus::Filled);

    let position = position.expect("position");
    assert_eq!(position.quantity, 40);
    assert_eq!(position.average_price, 1600);
    assert_eq!(position.current_price, Some(1500));
    assert_eq!(position.unrealized_pnl, Some(-4000));
    assert_eq!(position.notional_value, Some(60000));
    assert_eq!(position.realized_pnl, 0);

    let listed = listed.expect("list positions");
    assert_eq!(listed.summary.position_count, 1);
    assert_eq!(listed.summary.unpriced_count, 0);
    assert_eq!(listed.summary.total_unrealized_pnl, -4000);
}

#[tokio::test]
async fn test_executions_list_and_get() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "FEX").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    // Phase 1: rest a sell, cross it with a market buy, list executions, then fetch
    // the single execution by id.
    let rest = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 30,
            },
        )
        .await;
    let market = client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 30,
            },
        )
        .await;
    let list = client
        .list_executions(Some(&ExecutionsQuery {
            underlying: Some(underlying.clone()),
            ..Default::default()
        }))
        .await;
    let exec_id = list
        .as_ref()
        .ok()
        .and_then(|l| l.executions.first().map(|e| e.execution_id.clone()));
    let fetched = match &exec_id {
        Some(id) => Some(client.get_execution(id).await),
        None => None,
    };

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    rest.expect("rest ask");
    market.expect("market buy");
    let list = list.expect("list executions");
    assert_eq!(list.summary.total_executions, 1);
    assert_eq!(list.summary.total_volume, 30);
    let exec = list.executions.first().expect("one execution");
    assert_eq!(exec.side, OrderSide::Buy);
    assert_eq!(exec.price, 1500);
    assert_eq!(exec.quantity, 30);
    assert!(exec.symbol.contains(&underlying));

    let fetched = fetched
        .expect("execution should have been fetched")
        .expect("get execution");
    assert_eq!(fetched.execution_id, exec.execution_id);
    assert_eq!(fetched.price, 1500);
    assert_eq!(fetched.quantity, 30);
}

//! Position P&L and execution-report tests driven by the caller's own crossing
//! fills (bug #110 makes filling against market-maker quotes impossible, so all
//! liquidity here is self-provided).

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

    // Rest an ask at 1600 and a bid at 1400, then take 40 from the ask. The
    // resulting long (40 @ 1600) is marked at the 1500 mid, for a -4000 PnL.
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1600,
                quantity: 100,
            },
        )
        .await
        .expect("rest ask");
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 50,
            },
        )
        .await
        .expect("rest bid");

    let fill = client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 40,
            },
        )
        .await
        .expect("market buy");
    assert_eq!(fill.status, MarketOrderStatus::Filled);

    let symbol = format!("{underlying}-{TEST_EXPIRATION}-{TEST_STRIKE}-C");
    let position = client.get_position(&symbol).await.expect("position");
    assert_eq!(position.quantity, 40);
    assert_eq!(position.average_price, 1600);
    assert_eq!(position.current_price, Some(1500));
    assert_eq!(position.unrealized_pnl, Some(-4000));
    assert_eq!(position.notional_value, Some(60000));
    assert_eq!(position.realized_pnl, 0);

    let listed = client
        .list_positions(Some(&PositionQuery {
            underlying: Some(underlying.clone()),
        }))
        .await
        .expect("list positions");
    assert_eq!(listed.summary.position_count, 1);
    assert_eq!(listed.summary.unpriced_count, 0);
    assert_eq!(listed.summary.total_unrealized_pnl, -4000);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_executions_list_and_get() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "FEX").await;

    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 30,
            },
        )
        .await
        .expect("rest ask");
    client
        .submit_market_order(
            &place,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 30,
            },
        )
        .await
        .expect("market buy");

    let list = client
        .list_executions(Some(&ExecutionsQuery {
            underlying: Some(underlying.clone()),
            ..Default::default()
        }))
        .await
        .expect("list executions");
    assert_eq!(list.summary.total_executions, 1);
    assert_eq!(list.summary.total_volume, 30);
    let exec = list.executions.first().expect("one execution");
    assert_eq!(exec.side, OrderSide::Buy);
    assert_eq!(exec.price, 1500);
    assert_eq!(exec.quantity, 30);
    assert!(exec.symbol.contains(&underlying));

    let fetched = client
        .get_execution(&exec.execution_id)
        .await
        .expect("get execution");
    assert_eq!(fetched.execution_id, exec.execution_id);
    assert_eq!(fetched.price, 1500);
    assert_eq!(fetched.quantity, 30);

    cleanup_underlying(&client, &underlying).await;
}

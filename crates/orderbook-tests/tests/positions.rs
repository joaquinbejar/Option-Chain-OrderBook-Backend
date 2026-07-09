//! Position tracking tests.
//!
//! Issue #59: a position on a symbol with no current quote must be reported as
//! UNPRICED — the mark-dependent fields (`current_price`, `unrealized_pnl`,
//! `notional_value`) are omitted from the JSON rather than fabricated at a 0
//! mark, and the list summary excludes it from `total_unrealized_pnl` while
//! counting it in `unpriced_count`.

use orderbook_client::{
    AddOrderRequest, MarketOrderRequest, MarketOrderStatus, OptionPath, OrderSide, PositionQuery,
};
use orderbook_tests::{admin_client, cleanup_underlying, unique_symbol};

/// Opens a long position whose order book is empty after the fill (no bid, no
/// ask) and asserts the position is reported as unpriced everywhere.
#[tokio::test]
async fn test_unpriced_position_omits_mark_fields() {
    let client = admin_client().await.expect("admin client");
    let underlying = unique_symbol("UNP");
    let expiration = "20251231";
    let strike = 10000u64;

    // Setup the contract.
    client
        .create_underlying(&underlying)
        .await
        .expect("create underlying");
    client
        .create_expiration(&underlying, expiration)
        .await
        .expect("create expiration");
    client
        .create_strike(&underlying, expiration, strike)
        .await
        .expect("create strike");

    let option = OptionPath::call(&underlying, expiration, strike);

    // Provide exactly enough resting sell liquidity for the market buy so the
    // book is EMPTY (no bid, no ask) once the buy fully consumes it — the
    // resulting long position then has no current quote.
    client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 10,
            },
        )
        .await
        .expect("add resting sell");

    let result = client
        .submit_market_order(
            &option,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 10,
            },
        )
        .await
        .expect("market order fills");
    assert_eq!(result.status, MarketOrderStatus::Filled);
    assert_eq!(result.filled_quantity, 10);

    // The server records the position under `{underlying}-{expiration}-{strike}-C`.
    let symbol = format!("{}-{}-{}-C", underlying, expiration, strike);

    // GET /positions/{symbol}: unpriced -> the three mark fields are absent.
    let position = client.get_position(&symbol).await.expect("position found");
    assert_eq!(position.symbol, symbol);
    assert_eq!(position.quantity, 10);
    assert_eq!(
        position.current_price, None,
        "unpriced position must omit current_price"
    );
    assert_eq!(
        position.unrealized_pnl, None,
        "unpriced position must omit unrealized_pnl"
    );
    assert_eq!(
        position.notional_value, None,
        "unpriced position must omit notional_value"
    );

    // GET /positions filtered to this underlying: the position appears, still
    // unpriced, and the summary counts it as unpriced while excluding it from
    // total_unrealized_pnl.
    let listed = client
        .list_positions(Some(&PositionQuery {
            underlying: Some(underlying.clone()),
        }))
        .await
        .expect("list positions");
    let listed_pos = listed
        .positions
        .iter()
        .find(|p| p.symbol == symbol)
        .expect("position present in list");
    assert_eq!(listed_pos.current_price, None);
    assert_eq!(listed_pos.unrealized_pnl, None);
    assert_eq!(listed_pos.notional_value, None);
    assert_eq!(listed.summary.position_count, 1);
    assert_eq!(listed.summary.unpriced_count, 1);
    assert_eq!(listed.summary.total_unrealized_pnl, 0);

    // Clean up.
    cleanup_underlying(&client, &underlying).await;
}

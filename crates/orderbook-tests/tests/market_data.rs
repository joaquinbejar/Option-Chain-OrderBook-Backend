//! Market-data read endpoint tests: enriched snapshot, metrics, OHLC, last
//! trade, option chain, volatility surface, greeks, and strike/expiration detail.
//!
//! Reads that resolve a book by expiration use the server-formatted expiration;
//! the `last-trade` and `ohlc` stores are keyed by the raw request path, so those
//! use [`TEST_EXPIRATION`] (see the crate docs for bug #110). Greeks require a
//! priced underlying, so they run against the config-provisioned `BTC` book.
//!
//! Each test that creates an underlying uses the capture-then-assert pattern:
//! all requests (including the liquidity-setup helpers, which return a `Result`)
//! run first, then [`cleanup_underlying`], and only THEN the assertions — so a
//! failed assertion never leaks a test underlying.

use orderbook_client::{AddOrderRequest, Error, OhlcQuery, OptionPath, OrderSide, OrderbookClient};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, read_client, setup_underlying,
};

/// Rests a bid at 1400 and an ask at 1600 (both size 10) on the book. Returns the
/// setup outcome so the caller can defer the assertion until after cleanup.
async fn rest_two_sided(client: &OrderbookClient, underlying: &str) -> Result<(), Error> {
    let place = OptionPath::call(underlying, TEST_EXPIRATION, TEST_STRIKE);
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 10,
            },
        )
        .await?;
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1600,
                quantity: 10,
            },
        )
        .await?;
    Ok(())
}

/// Crosses a resting sell with a buy, producing one trade of `quantity` @ `price`.
/// Returns the setup outcome so the caller can defer the assertion until after
/// cleanup.
async fn cross_once(
    client: &OrderbookClient,
    underlying: &str,
    price: u128,
    quantity: u64,
) -> Result<(), Error> {
    let place = OptionPath::call(underlying, TEST_EXPIRATION, TEST_STRIKE);
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price,
                quantity,
            },
        )
        .await?;
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price,
                quantity,
            },
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn test_enriched_snapshot() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "SNP").await;
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);

    // Phase 1: rest liquidity, then read the snapshot at depth 10 and "full".
    let setup = rest_two_sided(&client, &underlying).await;
    let snapshot = client.get_option_snapshot(&read, Some("10")).await;
    let full = client.get_option_snapshot(&read, Some("full")).await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    setup.expect("rest two sided");
    let snapshot = snapshot.expect("enriched snapshot");
    assert!(snapshot.symbol.contains(&underlying));
    assert_eq!(snapshot.bids.len(), 1);
    assert_eq!(snapshot.asks.len(), 1);
    assert_eq!(snapshot.bids[0].price, 1400);
    assert_eq!(snapshot.asks[0].price, 1600);
    assert_eq!(snapshot.stats.mid_price, Some(1500.0));

    // "full" depth returns the same single level per side.
    let full = full.expect("full snapshot");
    assert_eq!(full.bids.len(), 1);
    assert_eq!(full.asks.len(), 1);
}

#[tokio::test]
async fn test_orderbook_metrics() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "MET").await;
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);

    // Phase 1.
    let setup = rest_two_sided(&client, &underlying).await;
    let metrics = client.get_orderbook_metrics(&read).await;

    // Phase 2.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3.
    setup.expect("rest two sided");
    let metrics = metrics.expect("orderbook metrics");
    assert!(metrics.symbol.contains(&underlying));
    assert_eq!(metrics.spread.current, Some(200));
    assert_eq!(metrics.depth.bid_depth_total, 10);
    assert_eq!(metrics.depth.ask_depth_total, 10);
    assert_eq!(metrics.prices.mid_price, Some(1500.0));
}

#[tokio::test]
async fn test_ohlc_from_fills() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "OHL").await;

    // Phase 1: cross once, then read OHLC (keyed by the raw request-path
    // expiration).
    let setup = cross_once(&client, &underlying, 1500, 40).await;
    let read = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let ohlc = client
        .get_ohlc(
            &read,
            Some(&OhlcQuery {
                interval: Some("1m".to_string()),
                ..Default::default()
            }),
        )
        .await;

    // Phase 2.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3.
    setup.expect("cross once");
    let ohlc = ohlc.expect("ohlc");
    assert_eq!(ohlc.interval, "1m");
    assert!(
        !ohlc.bars.is_empty(),
        "one fill must produce at least one bar"
    );
    let bar = ohlc.bars.last().expect("bar");
    assert_eq!(bar.open, 1500);
    assert_eq!(bar.high, 1500);
    assert_eq!(bar.low, 1500);
    assert_eq!(bar.close, 1500);
    assert_eq!(bar.volume, 40);
    assert_eq!(bar.trade_count, 1);
}

#[tokio::test]
async fn test_last_trade() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "LTR").await;

    // Phase 1: cross once, then read the last trade (keyed by the raw request-path
    // expiration).
    let setup = cross_once(&client, &underlying, 1550, 25).await;
    let read = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let trade = client.get_last_trade(&read).await;

    // Phase 2.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3.
    setup.expect("cross once");
    let trade = trade.expect("last trade");
    assert!(trade.symbol.contains(&underlying));
    assert_eq!(trade.price, 1550);
    assert_eq!(trade.quantity, 25);
    assert_eq!(trade.side, OrderSide::Buy);
    assert!(!trade.trade_id.is_empty());
}

#[tokio::test]
async fn test_option_chain() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "CHN").await;

    // Phase 1.
    let setup = rest_two_sided(&client, &underlying).await;
    let chain = client.get_option_chain(&underlying, &formatted).await;

    // Phase 2.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3.
    setup.expect("rest two sided");
    let chain = chain.expect("option chain");
    assert_eq!(chain.underlying, underlying);
    let row = chain
        .chain
        .iter()
        .find(|r| r.strike == TEST_STRIKE)
        .expect("strike row present");
    assert_eq!(row.call.bid, Some(1400));
    assert_eq!(row.call.ask, Some(1600));
}

#[tokio::test]
async fn test_volatility_surface() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "VOL").await;

    // Phase 1.
    let surface = client.get_volatility_surface(&underlying).await;

    // Phase 2.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3 — shape only; IVs may be null on a book with no priced quotes.
    let surface = surface.expect("volatility surface");
    assert_eq!(surface.underlying, underlying);
    assert!(surface.expirations.contains(&formatted));
    assert!(surface.strikes.contains(&TEST_STRIKE));
    assert!(surface.surface.contains_key(&formatted));
}

#[tokio::test]
async fn test_strike_and_expiration_details() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "DET").await;

    // Phase 1.
    let expiration = client.get_expiration(&underlying, &formatted).await;
    let strike = client
        .get_strike(&underlying, &formatted, TEST_STRIKE)
        .await;

    // Phase 2.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3.
    let expiration = expiration.expect("get expiration");
    assert!(expiration.strike_count >= 1);

    let strike = strike.expect("get strike");
    assert_eq!(strike.strike, TEST_STRIKE);
}

#[tokio::test]
async fn test_greeks_on_priced_underlying() {
    // Greeks require a spot price; use the config-provisioned BTC book, whose
    // DateTime expiration resolves by its canonical string directly. No underlying
    // is created here, so there is nothing to clean up.
    let client = read_client().await.expect("read client");

    let expiration = client
        .list_expirations("BTC")
        .await
        .expect("btc expirations")
        .expirations
        .into_iter()
        .next()
        .expect("btc has an expiration");
    let strikes = client
        .list_strikes("BTC", &expiration)
        .await
        .expect("btc strikes")
        .strikes;
    let strike = *strikes.get(strikes.len() / 2).expect("btc has strikes");

    let path = OptionPath::call("BTC", &expiration, strike);
    let greeks = client.get_option_greeks(&path).await.expect("greeks");

    assert!(greeks.symbol.contains("BTC"));
    assert!(greeks.iv.is_finite());
    assert!(greeks.theoretical_value.is_finite());
    assert!(greeks.greeks.delta.is_finite());
    assert!(greeks.timestamp_ms > 0);
}

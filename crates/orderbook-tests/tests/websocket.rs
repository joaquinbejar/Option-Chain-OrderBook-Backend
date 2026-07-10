//! WebSocket connection and message handling tests.
//!
//! The `/ws` upgrade is authenticated: the client appends the JWT as `?token=`
//! (see `OrderbookClient::ws_url`).

use orderbook_client::{
    AddOrderRequest, ChannelSubscription, MarketOrderRequest, MarketOrderStatus, OptionPath,
    OrderSide, SubscriptionChannel, WsClient, WsMessage,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, read_client, setup_underlying,
};
use std::time::Duration;

#[tokio::test]
async fn test_websocket_connection() {
    let client = read_client().await.expect("read client");

    let mut ws = WsClient::connect(&client.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    // Should receive a message (Connected first, or a heartbeat/broadcast).
    let timeout = tokio::time::timeout(Duration::from_secs(5), ws.recv()).await;

    match timeout {
        Ok(Some(msg)) => match msg {
            orderbook_client::WsMessage::Connected { message } => {
                assert!(message.contains("Connected"));
            }
            _ => {
                // Other messages (heartbeat, price, quote) are also valid.
            }
        },
        Ok(None) => panic!("WebSocket closed unexpectedly"),
        Err(_) => panic!("Timeout waiting for WebSocket message"),
    }
}

#[tokio::test]
async fn test_websocket_subscribe() {
    let client = read_client().await.expect("read client");

    let mut ws = WsClient::connect(&client.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    // Issue #86: assert the server actually ACKS the subscription — this
    // fails if the server stops confirming subscriptions, not merely if the
    // transport errors.
    // NOTE: the channel-less `subscribe` command is silently ignored by the
    // server (no registration, no ack) — which is exactly how the old
    // assert-nothing version of this test kept passing. Subscribe on the
    // real `orderbook` channel, which the server acks.
    let symbol = "BTC-20251231-100000-C";
    ws.subscribe_orderbook(symbol, None)
        .await
        .expect("Failed to send subscribe command");
    wait_for_ack(&mut ws, |msg| {
        matches!(msg, orderbook_client::WsMessage::Subscribed { symbol: s, .. } if s == symbol)
    })
    .await
    .expect("server must confirm the subscription");

    ws.unsubscribe_orderbook(symbol)
        .await
        .expect("Failed to send unsubscribe command");
    wait_for_ack(&mut ws, |msg| {
        matches!(msg, orderbook_client::WsMessage::Unsubscribed { symbol: s, .. } if s == symbol)
    })
    .await
    .expect("server must confirm the unsubscription");
}

/// Drains WS messages (quote/price events flow constantly) until `pred`
/// matches, or errors after a bounded timeout.
async fn wait_for_ack(
    ws: &mut WsClient,
    pred: impl Fn(&orderbook_client::WsMessage) -> bool,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for the expected WS message".to_string());
        }
        match tokio::time::timeout(remaining, ws.recv()).await {
            Ok(Some(msg)) if pred(&msg) => return Ok(()),
            Ok(Some(_)) => continue,
            Ok(None) => return Err("WS closed before the expected message".to_string()),
            Err(_) => return Err("timed out waiting for the expected WS message".to_string()),
        }
    }
}

#[tokio::test]
async fn test_websocket_heartbeat() {
    let client = read_client().await.expect("read client");

    let mut ws = WsClient::connect(&client.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    // Issue #86: the heartbeat fires on a fixed 30s cadence (issue #65), so
    // assert the actual Heartbeat variant arrives — with margin — instead of
    // merely proving the connection produced some message.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "no Heartbeat frame within 40s (cadence is 30s)"
        );
        match tokio::time::timeout(remaining, ws.recv()).await {
            Ok(Some(orderbook_client::WsMessage::Heartbeat { timestamp })) => {
                assert!(timestamp > 0, "heartbeat carries a real timestamp");
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("WS closed before a heartbeat arrived"),
            Err(_) => panic!("no Heartbeat frame within 40s (cadence is 30s)"),
        }
    }
}

/// Drains WS messages (quote/price/heartbeat noise flows constantly) until one
/// matches `pred`, returning it, or `None` after a bounded timeout / close.
async fn capture_msg(ws: &mut WsClient, pred: impl Fn(&WsMessage) -> bool) -> Option<WsMessage> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, ws.recv()).await {
            Ok(Some(msg)) if pred(&msg) => return Some(msg),
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => return None,
        }
    }
}

/// Issue #129: a REST book mutation must deliver an `orderbook_delta` to BOTH an
/// exact-symbol subscriber and a by-underlying wildcard subscriber.
#[tokio::test]
async fn test_orderbook_delta_delivered_exact_and_wildcard() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _fmt) = setup_underlying(&client, "WSD").await;
    // The server publishes deltas under the book's canonical symbol.
    let symbol = format!("{underlying}-{TEST_EXPIRATION}-{TEST_STRIKE}-C");

    // Two connections: one subscribes the EXACT symbol, one by-underlying
    // wildcard. A single REST mutation must reach both.
    let mut ws_exact = WsClient::connect(&client.ws_url())
        .await
        .expect("ws connect (exact)");
    let mut ws_wild = WsClient::connect(&client.ws_url())
        .await
        .expect("ws connect (wildcard)");

    // Batch-subscribe orderbook: exact symbol on one connection, wildcard on the
    // other. (Wildcard subscription is only expressible via batch_subscribe.)
    let exact_res = ws_exact
        .batch_subscribe(
            vec![ChannelSubscription {
                channel: SubscriptionChannel::Orderbook,
                symbol: Some(symbol.clone()),
                underlying: None,
                expiration: None,
                depth: None,
            }],
            Some("exact".to_string()),
        )
        .await;
    let wild_res = ws_wild
        .batch_subscribe(
            vec![ChannelSubscription {
                channel: SubscriptionChannel::Orderbook,
                symbol: None,
                underlying: Some(underlying.clone()),
                expiration: None,
                depth: None,
            }],
            Some("wild".to_string()),
        )
        .await;

    // Wait for both batch acks so the subscriptions are registered before the
    // mutation (a delta must not race ahead of registration).
    let exact_ack = wait_for_ack(&mut ws_exact, |m| {
        matches!(m, WsMessage::BatchSubscribed { .. })
    })
    .await;
    let wild_ack = wait_for_ack(&mut ws_wild, |m| {
        matches!(m, WsMessage::BatchSubscribed { .. })
    })
    .await;

    // REST: place a resting buy → a bid-side delta at that price.
    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let placed = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price: 1400,
                quantity: 7,
            },
        )
        .await;

    // Capture the delta on both connections (draining quote/heartbeat noise).
    let exact_delta = capture_msg(
        &mut ws_exact,
        |m| matches!(m, WsMessage::OrderbookDelta { symbol: s, .. } if *s == symbol),
    )
    .await;
    let wild_delta = capture_msg(
        &mut ws_wild,
        |m| matches!(m, WsMessage::OrderbookDelta { symbol: s, .. } if *s == symbol),
    )
    .await;

    // Cleanup before asserting so a failure never leaks a test underlying.
    cleanup_underlying(&client, &underlying).await;

    // Assert.
    exact_res.expect("batch subscribe (exact)");
    wild_res.expect("batch subscribe (wildcard)");
    exact_ack.expect("exact subscription ack");
    wild_ack.expect("wildcard subscription ack");
    placed.expect("add order");

    for (label, delta) in [("exact", exact_delta), ("wildcard", wild_delta)] {
        match delta {
            Some(WsMessage::OrderbookDelta {
                symbol: s, changes, ..
            }) => {
                assert_eq!(s, symbol, "{label} delta carries the mutated symbol");
                assert!(
                    changes
                        .iter()
                        .any(|c| c.side == "bid" && c.price == 1400 && c.quantity == 7),
                    "{label} delta must carry the resting bid level, got {changes:?}"
                );
            }
            other => panic!("{label} subscriber did not receive the delta: {other:?}"),
        }
    }
}

/// Issue #129: crossing your own resting order with a market order must deliver a
/// `trade` message with the exact execution price and quantity to a `trades`
/// subscriber.
#[tokio::test]
async fn test_trade_delivered_on_crossing_market_order() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _fmt) = setup_underlying(&client, "WST").await;
    let symbol = format!("{underlying}-{TEST_EXPIRATION}-{TEST_STRIKE}-C");
    let option = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);

    let mut ws = WsClient::connect(&client.ws_url())
        .await
        .expect("ws connect");
    let sub = ws.subscribe_trades(&symbol).await;
    let ack = wait_for_ack(&mut ws, |m| {
        matches!(m, WsMessage::Subscribed { channel, symbol: s } if channel == "trades" && *s == symbol)
    })
    .await;

    // Rest a sell, then cross it exactly with a market buy.
    let rest = client
        .add_order(
            &option,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1500,
                quantity: 5,
            },
        )
        .await;
    let market = client
        .submit_market_order(
            &option,
            &MarketOrderRequest {
                side: OrderSide::Buy,
                quantity: 5,
            },
        )
        .await;

    let trade = capture_msg(
        &mut ws,
        |m| matches!(m, WsMessage::Trade { symbol: s, .. } if *s == symbol),
    )
    .await;

    cleanup_underlying(&client, &underlying).await;

    sub.expect("subscribe trades");
    ack.expect("trades subscription ack");
    rest.expect("resting sell");
    let market = market.expect("market buy");
    assert_eq!(market.status, MarketOrderStatus::Filled);
    match trade {
        Some(WsMessage::Trade {
            symbol: s,
            price,
            quantity,
            ..
        }) => {
            assert_eq!(s, symbol);
            assert_eq!(price, 1500, "trade carries the exact execution price");
            assert_eq!(quantity, 5, "trade carries the exact execution quantity");
        }
        other => panic!("trades subscriber did not receive the trade: {other:?}"),
    }
}

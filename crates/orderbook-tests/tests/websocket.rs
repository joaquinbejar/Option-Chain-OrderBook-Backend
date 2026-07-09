//! WebSocket connection and message handling tests.
//!
//! The `/ws` upgrade is authenticated: the client appends the JWT as `?token=`
//! (see `OrderbookClient::ws_url`).

use orderbook_client::WsClient;
use orderbook_tests::read_client;
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

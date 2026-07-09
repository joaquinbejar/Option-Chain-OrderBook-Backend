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

    let ws = WsClient::connect(&client.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    ws.subscribe("BTC")
        .await
        .expect("Failed to send subscribe command");

    ws.unsubscribe("BTC")
        .await
        .expect("Failed to send unsubscribe command");
}

#[tokio::test]
async fn test_websocket_heartbeat() {
    let client = read_client().await.expect("read client");

    let mut ws = WsClient::connect(&client.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    // Verify the connection produces at least the initial message.
    let timeout = tokio::time::timeout(Duration::from_secs(2), ws.recv()).await;
    assert!(timeout.is_ok());
}

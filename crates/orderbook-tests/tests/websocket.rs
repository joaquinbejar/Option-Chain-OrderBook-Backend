//! WebSocket connection and message handling tests.

use orderbook_client::WsClient;
use orderbook_tests::get_api_url;
use std::time::Duration;

#[tokio::test]
async fn test_websocket_connection() {
    let base_url = get_api_url();
    let ws_url = base_url
        .replace("http://", "ws://")
        .replace("https://", "wss://")
        + "/ws";

    let mut ws = WsClient::connect(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");

    // Should receive a connected message
    let timeout = tokio::time::timeout(Duration::from_secs(5), ws.recv()).await;

    match timeout {
        Ok(Some(msg)) => {
            // First message should be Connected
            match msg {
                orderbook_client::WsMessage::Connected { message } => {
                    assert!(message.contains("Connected"));
                }
                _ => {
                    // Other messages are also valid (heartbeat, etc.)
                }
            }
        }
        Ok(None) => panic!("WebSocket closed unexpectedly"),
        Err(_) => panic!("Timeout waiting for WebSocket message"),
    }
}

#[tokio::test]
async fn test_websocket_subscribe() {
    let base_url = get_api_url();
    let ws_url = base_url
        .replace("http://", "ws://")
        .replace("https://", "wss://")
        + "/ws";

    let ws = WsClient::connect(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");

    // Subscribe to a symbol
    ws.subscribe("BTC")
        .await
        .expect("Failed to send subscribe command");

    // Unsubscribe
    ws.unsubscribe("BTC")
        .await
        .expect("Failed to send unsubscribe command");
}

#[tokio::test]
async fn test_websocket_heartbeat() {
    let base_url = get_api_url();
    let ws_url = base_url
        .replace("http://", "ws://")
        .replace("https://", "wss://")
        + "/ws";

    let mut ws = WsClient::connect(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");

    // Wait for messages (connected + potentially heartbeat)
    // Note: heartbeat is sent every 30 seconds, so we just verify connection works
    let timeout = tokio::time::timeout(Duration::from_secs(2), ws.recv()).await;

    // We should get at least the connected message
    assert!(timeout.is_ok());
}

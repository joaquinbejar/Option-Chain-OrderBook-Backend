//! Authentication and authorization integration tests.
//!
//! These run against a live server with `AUTH_BOOTSTRAP_SECRET` configured (see
//! `get_bootstrap_secret`). They exercise the authorized path plus the `401`
//! (missing / invalid token) and `403` (insufficient permission) paths.

use orderbook_client::{ClientCommand, Error, OrderbookClient, WsClient, WsMessage};
use orderbook_tests::{admin_client, create_test_client, get_api_url, read_client};
use std::time::Duration;

#[tokio::test]
async fn test_authorized_read_succeeds() {
    let client = read_client().await.expect("obtain read token");

    let stats = client
        .get_global_stats()
        .await
        .expect("authorized read should succeed");
    let _ = stats.underlying_count;
}

#[tokio::test]
async fn test_missing_token_is_unauthorized() {
    let client = create_test_client().expect("client builds");

    match client.get_global_stats().await {
        Err(Error::Api { status, .. }) => assert_eq!(status, 401),
        other => panic!("expected 401, got {other:?}"),
    }
}

#[tokio::test]
async fn test_invalid_token_is_unauthorized() {
    let client =
        OrderbookClient::with_token(&get_api_url(), "not.a.valid.jwt").expect("client builds");

    match client.get_global_stats().await {
        Err(Error::Api { status, .. }) => assert_eq!(status, 401),
        other => panic!("expected 401, got {other:?}"),
    }
}

#[tokio::test]
async fn test_insufficient_permission_is_forbidden() {
    // A read-only token cannot reach an admin-only controls endpoint.
    let client = read_client().await.expect("obtain read token");

    match client.get_controls().await {
        Err(Error::Api { status, .. }) => assert_eq!(status, 403),
        other => panic!("expected 403, got {other:?}"),
    }
}

#[tokio::test]
async fn test_admin_token_reaches_controls() {
    let client = admin_client().await.expect("obtain admin token");

    // Admin implies all, so the controls endpoint is reachable.
    let controls = client
        .get_controls()
        .await
        .expect("admin should reach controls");
    let _ = controls.master_enabled;
}

#[tokio::test]
async fn test_readonly_token_cannot_control_over_ws() {
    // A read-only token may open the WS but must NOT be able to run the
    // market-maker control commands (`kill`/`set_*`/`enable`).
    let client = read_client().await.expect("obtain read token");

    let mut ws = WsClient::connect(&client.ws_url())
        .await
        .expect("read token should connect to WS");

    ws.send(ClientCommand::kill())
        .await
        .expect("send kill command");

    // The server must reply with a forbidden error and not mutate state. Scan a
    // few messages (skipping the initial Connected / heartbeats) for the error.
    let mut saw_forbidden = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_secs(5), ws.recv()).await {
            Ok(Some(WsMessage::Error { message })) => {
                assert!(
                    message.contains("forbidden"),
                    "unexpected error message: {message}"
                );
                saw_forbidden = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(saw_forbidden, "expected a forbidden error for kill over WS");
}

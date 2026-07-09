//! Unit tests for client module.

use super::*;

// ============================================================================
// ClientConfig Tests
// ============================================================================

#[test]
fn test_client_config_default() {
    let config = ClientConfig::default();

    assert_eq!(config.base_url, "http://localhost:8080");
    assert_eq!(config.timeout, Duration::from_secs(30));
}

#[test]
fn test_client_config_custom() {
    let config = ClientConfig {
        base_url: "http://api.example.com:9000".to_string(),
        timeout: Duration::from_secs(60),
        token: None,
    };

    assert_eq!(config.base_url, "http://api.example.com:9000");
    assert_eq!(config.timeout, Duration::from_secs(60));
}

#[test]
fn test_client_config_clone() {
    let config = ClientConfig {
        base_url: "http://test.com".to_string(),
        timeout: Duration::from_secs(10),
        token: None,
    };

    let cloned = config.clone();
    assert_eq!(cloned.base_url, config.base_url);
    assert_eq!(cloned.timeout, config.timeout);
}

// ============================================================================
// OrderbookClient Creation Tests
// ============================================================================

#[test]
fn test_orderbook_client_new() {
    let config = ClientConfig::default();
    let client = OrderbookClient::new(config);

    assert!(client.is_ok());
}

#[test]
fn test_orderbook_client_with_base_url() {
    let client = OrderbookClient::with_base_url("http://localhost:3000");

    assert!(client.is_ok());
}

#[test]
fn test_orderbook_client_base_url_trimmed() {
    let client = OrderbookClient::with_base_url("http://localhost:8080/").unwrap();

    assert_eq!(client.ws_url(), "ws://localhost:8080/ws");
}

#[test]
fn test_orderbook_client_ws_url_http() {
    let client = OrderbookClient::with_base_url("http://localhost:8080").unwrap();

    assert_eq!(client.ws_url(), "ws://localhost:8080/ws");
}

#[test]
fn test_orderbook_client_ws_url_https() {
    let client = OrderbookClient::with_base_url("https://api.example.com").unwrap();

    assert_eq!(client.ws_url(), "wss://api.example.com/ws");
}

#[test]
fn test_orderbook_client_ws_url_with_token() {
    let client = OrderbookClient::with_token("http://localhost:8080", "abc.def.ghi").unwrap();

    assert_eq!(client.ws_url(), "ws://localhost:8080/ws?token=abc.def.ghi");
}

#[test]
fn test_orderbook_client_with_token_builds() {
    let client = OrderbookClient::with_token("http://localhost:8080", "abc.def.ghi");
    assert!(client.is_ok());
}

#[test]
fn test_orderbook_client_custom_timeout() {
    let config = ClientConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout: Duration::from_secs(5),
        token: None,
    };

    let client = OrderbookClient::new(config);
    assert!(client.is_ok());
}

// ============================================================================
// URL Building Tests
// ============================================================================

#[test]
fn test_option_path_url_building() {
    let path = OptionPath::call("AAPL", "20240315", 15000);

    assert_eq!(path.underlying, "AAPL");
    assert_eq!(path.expiration, "20240315");
    assert_eq!(path.strike, 15000);
    assert_eq!(path.style, "call");
}

#[test]
fn test_encode_segment_percent_encodes_reserved_characters() {
    // Ordinary segments pass through unchanged.
    assert_eq!(encode_segment("AAPL"), "AAPL");
    assert_eq!(encode_segment("20240315"), "20240315");
    assert_eq!(encode_segment("call"), "call");

    // Reserved / delimiter characters are percent-encoded so a caller-supplied
    // value cannot break out of its path segment or inject a new one.
    assert_eq!(encode_segment("a/b"), "a%2Fb");
    assert_eq!(encode_segment("a b"), "a%20b");
    assert_eq!(encode_segment("a?b"), "a%3Fb");
    assert_eq!(encode_segment("a#b"), "a%23b");
    // A traversal attempt stays a single, harmless segment.
    assert!(!encode_segment("../admin").contains('/'));
}

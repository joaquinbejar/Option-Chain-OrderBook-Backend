//! Unit tests for controls module.

use super::*;

// ============================================================================
// SystemControlResponse Tests
// ============================================================================

#[test]
fn test_system_control_response_serialization() {
    let response = SystemControlResponse {
        master_enabled: true,
        spread_multiplier: 1.5,
        size_scalar: 2.0,
        directional_skew: 0.1,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"master_enabled\":true"));
    assert!(json.contains("\"spread_multiplier\":1.5"));
    assert!(json.contains("\"size_scalar\":2.0"));
    assert!(json.contains("\"directional_skew\":0.1"));
}

#[test]
fn test_system_control_response_disabled() {
    let response = SystemControlResponse {
        master_enabled: false,
        spread_multiplier: 1.0,
        size_scalar: 1.0,
        directional_skew: 0.0,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"master_enabled\":false"));
}

// ============================================================================
// KillSwitchResponse Tests
// ============================================================================

#[test]
fn test_kill_switch_response_serialization() {
    let response = KillSwitchResponse {
        success: true,
        message: "Kill switch activated".to_string(),
        master_enabled: false,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"message\":\"Kill switch activated\""));
    assert!(json.contains("\"master_enabled\":false"));
}

#[test]
fn test_kill_switch_response_enable() {
    let response = KillSwitchResponse {
        success: true,
        message: "Quoting enabled".to_string(),
        master_enabled: true,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"master_enabled\":true"));
}

// ============================================================================
// UpdateParametersResponse Tests
// ============================================================================

#[test]
fn test_update_parameters_response_serialization() {
    let response = UpdateParametersResponse {
        success: true,
        spread_multiplier: 1.5,
        size_scalar: 200.0,
        directional_skew: 0.05,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"spread_multiplier\":1.5"));
    assert!(json.contains("\"size_scalar\":200.0"));
    assert!(json.contains("\"directional_skew\":0.05"));
}

// ============================================================================
// InstrumentToggleResponse Tests
// ============================================================================

#[test]
fn test_instrument_toggle_response_serialization() {
    let response = InstrumentToggleResponse {
        success: true,
        symbol: "AAPL".to_string(),
        enabled: true,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"symbol\":\"AAPL\""));
    assert!(json.contains("\"enabled\":true"));
}

#[test]
fn test_instrument_toggle_response_disabled() {
    let response = InstrumentToggleResponse {
        success: true,
        symbol: "SPY".to_string(),
        enabled: false,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"enabled\":false"));
}

// ============================================================================
// InsertPriceResponse Tests
// ============================================================================

#[test]
fn test_insert_price_response_serialization() {
    let response = InsertPriceResponse {
        success: true,
        symbol: "AAPL".to_string(),
        price_cents: 15050,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"symbol\":\"AAPL\""));
    assert!(json.contains("\"price_cents\":15050"));
    assert!(json.contains("\"timestamp\":\"2024-01-01T00:00:00Z\""));
}

// ============================================================================
// LatestPriceResponse Tests
// ============================================================================

#[test]
fn test_latest_price_response_serialization() {
    let response = LatestPriceResponse {
        symbol: "AAPL".to_string(),
        price: 150.50,
        bid: Some(150.40),
        ask: Some(150.60),
        volume: Some(1000000),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"symbol\":\"AAPL\""));
    assert!(json.contains("\"price\":150.5"));
    assert!(json.contains("\"bid\":150.4"));
    assert!(json.contains("\"ask\":150.6"));
    assert!(json.contains("\"volume\":1000000"));
}

#[test]
fn test_latest_price_response_minimal() {
    let response = LatestPriceResponse {
        symbol: "SPY".to_string(),
        price: 450.0,
        bid: None,
        ask: None,
        volume: None,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"symbol\":\"SPY\""));
    assert!(json.contains("\"price\":450.0"));
    assert!(json.contains("\"bid\":null"));
    assert!(json.contains("\"ask\":null"));
    assert!(json.contains("\"volume\":null"));
}

// ============================================================================
// InstrumentStatus Tests
// ============================================================================

#[test]
fn test_instrument_status_serialization() {
    let status = InstrumentStatus {
        symbol: "AAPL".to_string(),
        quoting_enabled: true,
        current_price: Some(150.50),
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"symbol\":\"AAPL\""));
    assert!(json.contains("\"quoting_enabled\":true"));
    assert!(json.contains("\"current_price\":150.5"));
}

#[test]
fn test_instrument_status_no_price() {
    let status = InstrumentStatus {
        symbol: "NEW".to_string(),
        quoting_enabled: false,
        current_price: None,
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"quoting_enabled\":false"));
    assert!(json.contains("\"current_price\":null"));
}

// ============================================================================
// InstrumentsListResponse Tests
// ============================================================================

#[test]
fn test_instruments_list_response_serialization() {
    let response = InstrumentsListResponse {
        instruments: vec![
            InstrumentStatus {
                symbol: "AAPL".to_string(),
                quoting_enabled: true,
                current_price: Some(150.50),
            },
            InstrumentStatus {
                symbol: "SPY".to_string(),
                quoting_enabled: false,
                current_price: Some(450.0),
            },
        ],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"instruments\":["));
    assert!(json.contains("\"symbol\":\"AAPL\""));
    assert!(json.contains("\"symbol\":\"SPY\""));
}

#[test]
fn test_instruments_list_response_empty() {
    let response = InstrumentsListResponse {
        instruments: vec![],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"instruments\":[]"));
}

// ============================================================================
// dollars_to_cents Helper Tests
// ============================================================================

#[test]
fn test_dollars_to_cents_valid_conversion() {
    // 123.45 dollars -> 12345 cents (issue acceptance criterion).
    assert_eq!(dollars_to_cents("price", 123.45).unwrap(), 12345);
    assert_eq!(dollars_to_cents("price", 0.0).unwrap(), 0);
    assert_eq!(dollars_to_cents("price", 1.0).unwrap(), 100);
    assert_eq!(dollars_to_cents("price", 150.50).unwrap(), 15050);
    // Sub-cent rounding to the nearest cent.
    assert_eq!(dollars_to_cents("price", 0.005).unwrap(), 1);
    assert_eq!(dollars_to_cents("price", 0.004).unwrap(), 0);
}

#[test]
fn test_dollars_to_cents_rejects_negative() {
    let err = dollars_to_cents("price", -0.01).unwrap_err();
    assert!(matches!(err, ApiError::InvalidRequest(_)));
    // Error message carries the offending field name and value, no secrets.
    let msg = err.to_string();
    assert!(msg.contains("price"));
    assert!(msg.contains("-0.01"));
}

#[test]
fn test_dollars_to_cents_rejects_nan() {
    let err = dollars_to_cents("bid", f64::NAN).unwrap_err();
    assert!(matches!(err, ApiError::InvalidRequest(_)));
    assert!(err.to_string().contains("bid"));
}

#[test]
fn test_dollars_to_cents_rejects_infinite() {
    assert!(matches!(
        dollars_to_cents("ask", f64::INFINITY),
        Err(ApiError::InvalidRequest(_))
    ));
    assert!(matches!(
        dollars_to_cents("ask", f64::NEG_INFINITY),
        Err(ApiError::InvalidRequest(_))
    ));
}

#[test]
fn test_dollars_to_cents_rejects_too_large() {
    let err = dollars_to_cents("price", MAX_PRICE_DOLLARS * 10.0).unwrap_err();
    assert!(matches!(err, ApiError::InvalidRequest(_)));
    assert!(err.to_string().contains("price"));
    // The cap itself is still accepted (and never overflows u64/i64).
    let cents = dollars_to_cents("price", MAX_PRICE_DOLLARS).unwrap();
    assert!(cents_to_i64("price", cents).is_ok());
}

// ============================================================================
// insert_price Handler Validation Tests
// ============================================================================

fn insert_price_request(price: f64, bid: Option<f64>, ask: Option<f64>) -> InsertPriceRequest {
    InsertPriceRequest {
        symbol: "TEST".to_string(),
        price,
        bid,
        ask,
        volume: None,
        source: Some("unit-test".to_string()),
    }
}

#[tokio::test]
async fn test_insert_price_valid_succeeds() {
    let state = Arc::new(AppState::new());
    let req = insert_price_request(123.45, Some(123.40), Some(123.50));

    let resp = insert_price(State(Arc::clone(&state)), Json(req))
        .await
        .expect("valid positive price should succeed");

    // 123.45 dollars -> 12345 cents, reflected in the response and the
    // in-memory market maker.
    assert!(resp.0.success);
    assert_eq!(resp.0.symbol, "TEST");
    assert_eq!(resp.0.price_cents, 12345);
    assert_eq!(state.market_maker.get_price("TEST"), Some(12345));
}

#[tokio::test]
async fn test_insert_price_negative_rejected_no_update() {
    let state = Arc::new(AppState::new());
    let req = insert_price_request(-1.0, None, None);

    let result = insert_price(State(Arc::clone(&state)), Json(req)).await;

    assert!(matches!(result, Err(ApiError::InvalidRequest(_))));
    // Market maker (and, by ordering, the DB) is never touched.
    assert_eq!(state.market_maker.get_price("TEST"), None);
}

#[tokio::test]
async fn test_insert_price_nan_rejected_no_update() {
    let state = Arc::new(AppState::new());
    let req = insert_price_request(f64::NAN, None, None);

    let result = insert_price(State(Arc::clone(&state)), Json(req)).await;

    assert!(matches!(result, Err(ApiError::InvalidRequest(_))));
    assert_eq!(state.market_maker.get_price("TEST"), None);
}

#[tokio::test]
async fn test_insert_price_infinite_rejected_no_update() {
    let state = Arc::new(AppState::new());
    let req = insert_price_request(f64::INFINITY, None, None);

    let result = insert_price(State(Arc::clone(&state)), Json(req)).await;

    assert!(matches!(result, Err(ApiError::InvalidRequest(_))));
    assert_eq!(state.market_maker.get_price("TEST"), None);
}

#[tokio::test]
async fn test_insert_price_absurdly_large_rejected_no_update() {
    let state = Arc::new(AppState::new());
    let req = insert_price_request(MAX_PRICE_DOLLARS * 100.0, None, None);

    let result = insert_price(State(Arc::clone(&state)), Json(req)).await;

    assert!(matches!(result, Err(ApiError::InvalidRequest(_))));
    assert_eq!(state.market_maker.get_price("TEST"), None);
}

#[tokio::test]
async fn test_insert_price_bad_bid_rejected_all_or_nothing() {
    // The price is valid but the bid is invalid: the whole request is rejected
    // and the (valid) price is NOT applied to the market maker.
    for bad_bid in [-5.0, f64::NAN, f64::INFINITY, MAX_PRICE_DOLLARS * 2.0] {
        let state = Arc::new(AppState::new());
        let req = insert_price_request(100.0, Some(bad_bid), None);

        let result = insert_price(State(Arc::clone(&state)), Json(req)).await;

        assert!(
            matches!(result, Err(ApiError::InvalidRequest(_))),
            "bid {bad_bid} should be rejected"
        );
        assert_eq!(
            state.market_maker.get_price("TEST"),
            None,
            "market maker must not be updated when bid {bad_bid} is invalid"
        );
    }
}

#[tokio::test]
async fn test_insert_price_bad_ask_rejected_all_or_nothing() {
    for bad_ask in [-5.0, f64::NAN, f64::NEG_INFINITY, MAX_PRICE_DOLLARS * 2.0] {
        let state = Arc::new(AppState::new());
        let req = insert_price_request(100.0, None, Some(bad_ask));

        let result = insert_price(State(Arc::clone(&state)), Json(req)).await;

        assert!(
            matches!(result, Err(ApiError::InvalidRequest(_))),
            "ask {bad_ask} should be rejected"
        );
        assert_eq!(
            state.market_maker.get_price("TEST"),
            None,
            "market maker must not be updated when ask {bad_ask} is invalid"
        );
    }
}

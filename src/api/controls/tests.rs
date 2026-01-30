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

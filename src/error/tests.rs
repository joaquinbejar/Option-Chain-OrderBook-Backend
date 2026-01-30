//! Unit tests for error module.

use super::*;

// ============================================================================
// ErrorResponse Tests
// ============================================================================

#[test]
fn test_error_response_serialization() {
    let response = ErrorResponse {
        error: "Something went wrong".to_string(),
        code: "INTERNAL_ERROR".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"error\":\"Something went wrong\""));
    assert!(json.contains("\"code\":\"INTERNAL_ERROR\""));
}

// ============================================================================
// RateLimitErrorResponse Tests
// ============================================================================

#[test]
fn test_rate_limit_error_response_serialization() {
    let response = RateLimitErrorResponse {
        error: "Rate limit exceeded".to_string(),
        code: "RATE_LIMIT_EXCEEDED".to_string(),
        limit: 100,
        remaining: 0,
        reset: 1704067260,
        retry_after: 60,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"error\":\"Rate limit exceeded\""));
    assert!(json.contains("\"code\":\"RATE_LIMIT_EXCEEDED\""));
    assert!(json.contains("\"limit\":100"));
    assert!(json.contains("\"remaining\":0"));
    assert!(json.contains("\"reset\":1704067260"));
    assert!(json.contains("\"retry_after\":60"));
}

// ============================================================================
// ApiError Display Tests
// ============================================================================

#[test]
fn test_api_error_underlying_not_found_display() {
    let error = ApiError::UnderlyingNotFound("AAPL".to_string());
    assert_eq!(format!("{}", error), "Underlying not found: AAPL");
}

#[test]
fn test_api_error_expiration_not_found_display() {
    let error = ApiError::ExpirationNotFound("20240315".to_string());
    assert_eq!(format!("{}", error), "Expiration not found: 20240315");
}

#[test]
fn test_api_error_strike_not_found_display() {
    let error = ApiError::StrikeNotFound(15000);
    assert_eq!(format!("{}", error), "Strike not found: 15000");
}

#[test]
fn test_api_error_invalid_request_display() {
    let error = ApiError::InvalidRequest("Missing required field".to_string());
    assert_eq!(
        format!("{}", error),
        "Invalid request: Missing required field"
    );
}

#[test]
fn test_api_error_internal_display() {
    let error = ApiError::Internal("Database connection failed".to_string());
    assert_eq!(
        format!("{}", error),
        "Internal server error: Database connection failed"
    );
}

#[test]
fn test_api_error_orderbook_display() {
    let error = ApiError::OrderBook("Order not found".to_string());
    assert_eq!(format!("{}", error), "OrderBook error: Order not found");
}

#[test]
fn test_api_error_database_display() {
    let error = ApiError::Database("Connection timeout".to_string());
    assert_eq!(format!("{}", error), "Database error: Connection timeout");
}

#[test]
fn test_api_error_not_found_display() {
    let error = ApiError::NotFound("Resource does not exist".to_string());
    assert_eq!(format!("{}", error), "Not found: Resource does not exist");
}

#[test]
fn test_api_error_rate_limit_exceeded_display() {
    let error = ApiError::RateLimitExceeded {
        limit: 100,
        remaining: 0,
        reset: 1704067260,
        retry_after: 60,
    };
    assert_eq!(format!("{}", error), "Rate limit exceeded");
}

// ============================================================================
// ApiError IntoResponse Tests
// ============================================================================

#[test]
fn test_api_error_underlying_not_found_into_response() {
    let error = ApiError::UnderlyingNotFound("AAPL".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_api_error_expiration_not_found_into_response() {
    let error = ApiError::ExpirationNotFound("20240315".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_api_error_strike_not_found_into_response() {
    let error = ApiError::StrikeNotFound(15000);
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_api_error_invalid_request_into_response() {
    let error = ApiError::InvalidRequest("Bad input".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_api_error_internal_into_response() {
    let error = ApiError::Internal("Server error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_api_error_orderbook_into_response() {
    let error = ApiError::OrderBook("Order error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_api_error_database_into_response() {
    let error = ApiError::Database("DB error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_api_error_not_found_into_response() {
    let error = ApiError::NotFound("Not found".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_api_error_rate_limit_exceeded_into_response() {
    let error = ApiError::RateLimitExceeded {
        limit: 100,
        remaining: 0,
        reset: 1704067260,
        retry_after: 60,
    };
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ============================================================================
// ApiError Debug Tests
// ============================================================================

#[test]
fn test_api_error_debug() {
    let error = ApiError::UnderlyingNotFound("AAPL".to_string());
    let debug = format!("{:?}", error);
    assert!(debug.contains("UnderlyingNotFound"));
    assert!(debug.contains("AAPL"));
}

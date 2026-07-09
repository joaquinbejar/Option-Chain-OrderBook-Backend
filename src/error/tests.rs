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
    assert_eq!(format!("{}", error), "underlying not found: AAPL");
}

#[test]
fn test_api_error_expiration_not_found_display() {
    let error = ApiError::ExpirationNotFound("20240315".to_string());
    assert_eq!(format!("{}", error), "expiration not found: 20240315");
}

#[test]
fn test_api_error_strike_not_found_display() {
    let error = ApiError::StrikeNotFound(15000);
    assert_eq!(format!("{}", error), "strike not found: 15000");
}

#[test]
fn test_api_error_invalid_request_display() {
    let error = ApiError::InvalidRequest("Missing required field".to_string());
    assert_eq!(
        format!("{}", error),
        "invalid request: Missing required field"
    );
}

#[test]
fn test_api_error_internal_display() {
    let error = ApiError::Internal("Database connection failed".to_string());
    assert_eq!(
        format!("{}", error),
        "internal server error: Database connection failed"
    );
}

#[test]
fn test_api_error_orderbook_display() {
    let error = ApiError::OrderBook("Order not found".to_string());
    assert_eq!(format!("{}", error), "orderbook error: Order not found");
}

#[test]
fn test_api_error_database_display() {
    let error = ApiError::Database("Connection timeout".to_string());
    assert_eq!(format!("{}", error), "database error: Connection timeout");
}

#[test]
fn test_api_error_not_found_display() {
    let error = ApiError::NotFound("Resource does not exist".to_string());
    assert_eq!(format!("{}", error), "not found: Resource does not exist");
}

#[test]
fn test_api_error_rate_limit_exceeded_display() {
    let error = ApiError::RateLimitExceeded {
        limit: 100,
        remaining: 0,
        reset: 1704067260,
        retry_after: 60,
    };
    assert_eq!(format!("{}", error), "rate limit exceeded");
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

/// Reads the full response body into a UTF-8 string for assertions.
async fn body_to_string(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(bytes.to_vec()).expect("response body should be valid UTF-8")
}

#[tokio::test]
async fn test_api_error_internal_into_response() {
    // The inner detail must NOT leak into the client body; the response is a
    // fixed, generic message and a 500 status.
    let detail = "connection to host db.internal failed";
    let error = ApiError::Internal(detail.to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = body_to_string(response).await;
    assert!(body.contains("\"error\":\"internal server error\""));
    assert!(body.contains("\"code\":\"INTERNAL_ERROR\""));
    assert!(
        !body.contains(detail),
        "5xx body must not echo the inner detail: {body}"
    );
}

#[test]
fn test_api_error_orderbook_into_response() {
    let error = ApiError::OrderBook("Order error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_api_error_database_into_response() {
    // The inner sqlx detail must NOT leak into the client body; the response is
    // a fixed, generic message and a 500 status.
    let detail = "error returned from database: column accounts.secret does not exist";
    let error = ApiError::Database(detail.to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = body_to_string(response).await;
    assert!(body.contains("\"error\":\"database error\""));
    assert!(body.contains("\"code\":\"DATABASE_ERROR\""));
    assert!(
        !body.contains(detail),
        "5xx body must not echo the inner detail: {body}"
    );
}

#[tokio::test]
async fn test_database_error_does_not_leak_sensitive_detail() {
    // A realistic sqlx error string carrying a table/column name and host detail
    // must never appear in the response body sent to the client.
    let sensitive =
        "error returned from database: relation \"users_api_keys\" host=10.0.0.5 dbname=prod";
    let error = ApiError::Database(sensitive.to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = body_to_string(response).await;
    assert!(
        !body.contains("users_api_keys"),
        "table name leaked into body: {body}"
    );
    assert!(!body.contains("10.0.0.5"), "host leaked into body: {body}");
    assert!(
        !body.contains("dbname=prod"),
        "dbname leaked into body: {body}"
    );
    assert_eq!(
        body,
        "{\"error\":\"database error\",\"code\":\"DATABASE_ERROR\"}"
    );
}

#[tokio::test]
async fn test_internal_error_does_not_leak_sensitive_detail() {
    // The inner Internal detail (which may wrap arbitrary lower-level errors)
    // must never reach the client body.
    let sensitive = "panic at src/db/pool.rs: DATABASE_URL=postgres://user:pass@host/db";
    let error = ApiError::Internal(sensitive.to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = body_to_string(response).await;
    assert!(
        !body.contains("postgres://"),
        "connection string leaked into body: {body}"
    );
    assert_eq!(
        body,
        "{\"error\":\"internal server error\",\"code\":\"INTERNAL_ERROR\"}"
    );
}

#[test]
fn test_api_error_not_found_into_response() {
    let error = ApiError::NotFound("Not found".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_api_error_unauthorized_into_response() {
    let error = ApiError::Unauthorized("missing token".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_api_error_forbidden_into_response() {
    let error = ApiError::Forbidden("requires admin".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_api_error_unauthorized_display() {
    let error = ApiError::Unauthorized("missing token".to_string());
    assert_eq!(format!("{}", error), "unauthorized: missing token");
}

#[test]
fn test_api_error_forbidden_display() {
    let error = ApiError::Forbidden("requires admin".to_string());
    assert_eq!(format!("{}", error), "forbidden: requires admin");
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

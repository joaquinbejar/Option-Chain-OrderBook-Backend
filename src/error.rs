//! Error types for the REST API.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// API error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
    /// Error code.
    pub code: String,
}

/// API error types.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Underlying not found.
    #[error("Underlying not found: {0}")]
    UnderlyingNotFound(String),

    /// Expiration not found.
    #[error("Expiration not found: {0}")]
    ExpirationNotFound(String),

    /// Strike not found.
    #[error("Strike not found: {0}")]
    StrikeNotFound(u64),

    /// Order not found.
    #[error("Order not found: {0}")]
    OrderNotFound(String),

    /// Invalid request.
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Internal server error.
    #[error("Internal server error: {0}")]
    Internal(String),

    /// OrderBook error.
    #[error("OrderBook error: {0}")]
    OrderBook(String),

    /// Database error.
    #[error("Database error: {0}")]
    Database(String),

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::UnderlyingNotFound(_) => (StatusCode::NOT_FOUND, "UNDERLYING_NOT_FOUND"),
            ApiError::ExpirationNotFound(_) => (StatusCode::NOT_FOUND, "EXPIRATION_NOT_FOUND"),
            ApiError::StrikeNotFound(_) => (StatusCode::NOT_FOUND, "STRIKE_NOT_FOUND"),
            ApiError::OrderNotFound(_) => (StatusCode::NOT_FOUND, "ORDER_NOT_FOUND"),
            ApiError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "INVALID_REQUEST"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
            ApiError::OrderBook(_) => (StatusCode::BAD_REQUEST, "ORDERBOOK_ERROR"),
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        };

        let body = Json(ErrorResponse {
            error: self.to_string(),
            code: code.to_string(),
        });

        (status, body).into_response()
    }
}

impl From<option_chain_orderbook::Error> for ApiError {
    fn from(err: option_chain_orderbook::Error) -> Self {
        ApiError::OrderBook(err.to_string())
    }
}

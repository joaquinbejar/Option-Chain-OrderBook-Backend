//! Error types for the REST API.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

#[cfg(test)]
mod tests;

/// API error response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
    /// Error code.
    pub code: String,
}

/// Rate limit error response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct RateLimitErrorResponse {
    /// Error message.
    pub error: String,
    /// Error code.
    pub code: String,
    /// Maximum requests allowed.
    pub limit: u32,
    /// Remaining requests.
    pub remaining: u32,
    /// Unix timestamp when the rate limit resets.
    pub reset: u64,
    /// Seconds until reset.
    pub retry_after: u64,
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

    /// Authentication failed (missing, malformed, or expired token).
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// The authenticated caller lacks the required permission.
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimitExceeded {
        /// Maximum requests allowed.
        limit: u32,
        /// Remaining requests (always 0 when exceeded).
        remaining: u32,
        /// Unix timestamp when the rate limit resets.
        reset: u64,
        /// Seconds until reset.
        retry_after: u64,
    },
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match &self {
            ApiError::RateLimitExceeded {
                limit,
                remaining,
                reset,
                retry_after,
            } => {
                let body = Json(RateLimitErrorResponse {
                    error: "Rate limit exceeded".to_string(),
                    code: "RATE_LIMIT_EXCEEDED".to_string(),
                    limit: *limit,
                    remaining: *remaining,
                    reset: *reset,
                    retry_after: *retry_after,
                });

                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [
                        ("X-RateLimit-Limit", limit.to_string()),
                        ("X-RateLimit-Remaining", remaining.to_string()),
                        ("X-RateLimit-Reset", reset.to_string()),
                        ("Retry-After", retry_after.to_string()),
                    ],
                    body,
                )
                    .into_response()
            }
            _ => {
                // For 5xx variants (`Internal`, `Database`) the inner string is
                // built from lower-level / sqlx errors that can carry host,
                // database, table, column, or constraint names. We log the full
                // detail server-side at ERROR and return a FIXED, generic body so
                // no internal detail ever reaches the client. The 4xx variants
                // carry safe, useful domain values and keep their detailed body.
                let (status, code, error) = match &self {
                    ApiError::UnderlyingNotFound(_) => (
                        StatusCode::NOT_FOUND,
                        "UNDERLYING_NOT_FOUND",
                        self.to_string(),
                    ),
                    ApiError::ExpirationNotFound(_) => (
                        StatusCode::NOT_FOUND,
                        "EXPIRATION_NOT_FOUND",
                        self.to_string(),
                    ),
                    ApiError::StrikeNotFound(_) => {
                        (StatusCode::NOT_FOUND, "STRIKE_NOT_FOUND", self.to_string())
                    }
                    ApiError::InvalidRequest(_) => {
                        (StatusCode::BAD_REQUEST, "INVALID_REQUEST", self.to_string())
                    }
                    ApiError::OrderBook(_) => {
                        (StatusCode::BAD_REQUEST, "ORDERBOOK_ERROR", self.to_string())
                    }
                    ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND", self.to_string()),
                    ApiError::Unauthorized(_) => {
                        (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", self.to_string())
                    }
                    ApiError::Forbidden(_) => {
                        (StatusCode::FORBIDDEN, "FORBIDDEN", self.to_string())
                    }
                    ApiError::Internal(_) => {
                        tracing::error!(
                            code = "INTERNAL_ERROR",
                            detail = %self,
                            "internal server error"
                        );
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "INTERNAL_ERROR",
                            "internal server error".to_string(),
                        )
                    }
                    ApiError::Database(_) => {
                        tracing::error!(
                            code = "DATABASE_ERROR",
                            detail = %self,
                            "database error"
                        );
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "DATABASE_ERROR",
                            "database error".to_string(),
                        )
                    }
                    ApiError::RateLimitExceeded { .. } => unreachable!(),
                };

                let body = Json(ErrorResponse {
                    error,
                    code: code.to_string(),
                });

                (status, body).into_response()
            }
        }
    }
}

impl From<option_chain_orderbook::Error> for ApiError {
    fn from(err: option_chain_orderbook::Error) -> Self {
        ApiError::OrderBook(err.to_string())
    }
}

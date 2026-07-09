//! Error types for the orderbook client.

use thiserror::Error;

#[cfg(test)]
mod tests;

/// Client error types.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// WebSocket error.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] Box<tokio_tungstenite::tungstenite::Error>),

    /// API returned an error response.
    #[error("API error ({status}): {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message from API.
        message: String,
    },

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid request parameters.
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Connection closed unexpectedly.
    #[error("Connection closed")]
    ConnectionClosed,
}

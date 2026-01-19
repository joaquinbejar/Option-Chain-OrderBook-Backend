//! HTTP client library for Option Chain OrderBook API.
//!
//! This crate provides a typed HTTP client for interacting with the Option Chain
//! OrderBook backend API. It supports all REST endpoints and WebSocket connections.
//!
//! # Example
//!
//! ```no_run
//! use orderbook_client::{OrderbookClient, ClientConfig};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), orderbook_client::Error> {
//!     let client = OrderbookClient::new(ClientConfig {
//!         base_url: "http://localhost:8080".into(),
//!         timeout: Duration::from_secs(30),
//!     });
//!
//!     // Check health
//!     let health = client.health_check().await?;
//!     println!("Status: {}", health.status);
//!
//!     Ok(())
//! }
//! ```

mod client;
mod error;
mod types;
mod websocket;

pub use client::{ClientConfig, OrderbookClient};
pub use error::Error;
pub use types::*;
pub use websocket::{WsClient, WsMessage};

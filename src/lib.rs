//! # Option Chain OrderBook Backend - REST API Server
//!
//! A high-performance REST API backend for interacting with the
//! [Option Chain OrderBook](https://crates.io/crates/option-chain-orderbook) library.
//! Built with [Axum](https://crates.io/crates/axum) for async HTTP handling and
//! provides OpenAPI/Swagger documentation via [utoipa](https://crates.io/crates/utoipa).
//!
//! ## Key Features
//!
//! - **RESTful API**: Full CRUD operations for the option chain orderbook hierarchy.
//!
//! - **Hierarchical Access**: Navigate from underlying assets down to individual
//!   option order books through a clean URL structure.
//!
//! - **Real-time WebSocket**: Subscribe to orderbook updates, trades, and market data.
//!
//! - **Market Making Engine**: Built-in market maker with configurable spread,
//!   size, and skew parameters.
//!
//! - **Position Tracking**: Track positions and P&L across all instruments.
//!
//! - **Execution Reports**: Complete audit trail of all trade executions.
//!
//! - **Orderbook Persistence**: Snapshot and restore orderbook state.
//!
//! - **Rate Limiting**: Configurable per-key rate limiting for API access.
//!
//! - **API Key Authentication**: Secure API access with permission-based keys.
//!
//! - **OpenAPI Documentation**: Auto-generated Swagger UI for API exploration
//!   and testing at `/swagger-ui/`.
//!
//! - **CORS Support**: Cross-origin resource sharing enabled for frontend integration.
//!
//! - **Structured Logging**: Request tracing with `tower-http` for debugging
//!   and monitoring.
//!
//! ## Architecture
//!
//! The API follows the hierarchical structure of the Option Chain OrderBook library:
//!
//! ```text
//! /api/v1/underlyings                                    → UnderlyingOrderBookManager
//!   └── /api/v1/underlyings/{underlying}                 → UnderlyingOrderBook
//!         └── /expirations                               → ExpirationOrderBookManager
//!               └── /expirations/{expiration}            → ExpirationOrderBook
//!                     └── /strikes                       → StrikeOrderBookManager
//!                           └── /strikes/{strike}        → StrikeOrderBook
//!                                 └── /options/{style}   → OptionOrderBook
//! ```
//!
//! ## Module Structure
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`api`] | Route handlers, WebSocket, and router configuration |
//! | [`auth`] | API key authentication and rate limiting |
//! | [`config`] | Server and market maker configuration |
//! | [`db`] | Database connection pool and schema |
//! | [`error`] | API error types with `IntoResponse` implementation |
//! | [`market_maker`] | Market making engine with pricing and quoting |
//! | [`models`] | Request/response DTOs with OpenAPI schemas |
//! | [`ohlc`] | OHLC candlestick aggregation |
//! | [`simulation`] | Price simulation for testing |
//! | [`state`] | Application state management |
//!
//! ## API Endpoints
//!
//! ### Health & Statistics
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/health` | Health check |
//! | GET | `/api/v1/stats` | Global statistics |
//!
//! ### Authentication
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | POST | `/api/v1/auth/keys` | Create API key |
//! | GET | `/api/v1/auth/keys` | List API keys |
//! | DELETE | `/api/v1/auth/keys/{key_id}` | Delete API key |
//!
//! ### Controls (Market Maker)
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/controls` | Get system control status |
//! | POST | `/api/v1/controls/kill-switch` | Disable all quoting |
//! | POST | `/api/v1/controls/enable` | Enable quoting |
//! | POST | `/api/v1/controls/parameters` | Update spread/size/skew |
//! | GET | `/api/v1/controls/instruments` | List instruments |
//! | POST | `/api/v1/controls/instrument/{symbol}/toggle` | Toggle instrument |
//!
//! ### Prices
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | POST | `/api/v1/prices` | Insert underlying price |
//! | GET | `/api/v1/prices` | Get all prices |
//! | GET | `/api/v1/prices/{symbol}` | Get latest price |
//!
//! ### Underlyings
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/underlyings` | List all underlyings |
//! | POST | `/api/v1/underlyings/{underlying}` | Create or get underlying |
//! | GET | `/api/v1/underlyings/{underlying}` | Get underlying details |
//! | DELETE | `/api/v1/underlyings/{underlying}` | Delete underlying |
//!
//! ### Expirations
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/underlyings/{underlying}/expirations` | List expirations |
//! | POST | `/api/v1/underlyings/{underlying}/expirations/{exp}` | Create expiration |
//! | GET | `/api/v1/underlyings/{underlying}/expirations/{exp}` | Get expiration |
//!
//! ### Volatility Surface
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/underlyings/{underlying}/volatility-surface` | Get IV surface |
//!
//! ### Option Chain
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `.../expirations/{exp}/chain` | Get option chain matrix |
//!
//! ### Strikes
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `.../expirations/{exp}/strikes` | List strikes |
//! | POST | `.../expirations/{exp}/strikes/{strike}` | Create strike |
//! | GET | `.../expirations/{exp}/strikes/{strike}` | Get strike details |
//!
//! ### Options
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `.../options/{style}` | Get option book |
//! | POST | `.../options/{style}/orders` | Add limit order |
//! | POST | `.../options/{style}/orders/market` | Submit market order |
//! | DELETE | `.../options/{style}/orders/{id}` | Cancel order |
//! | PATCH | `.../options/{style}/orders/{id}` | Modify order |
//! | GET | `.../options/{style}/quote` | Get quote |
//! | GET | `.../options/{style}/greeks` | Get option greeks |
//! | GET | `.../options/{style}/snapshot` | Get enriched snapshot |
//! | GET | `.../options/{style}/last-trade` | Get last trade |
//! | GET | `.../options/{style}/ohlc` | Get OHLC bars |
//! | GET | `.../options/{style}/metrics` | Get orderbook metrics |
//!
//! ### Orders
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/orders` | List all orders |
//! | GET | `/api/v1/orders/{order_id}` | Get order status |
//! | POST | `/api/v1/orders/bulk` | Bulk submit orders |
//! | DELETE | `/api/v1/orders/bulk` | Bulk cancel orders |
//! | DELETE | `/api/v1/orders/cancel-all` | Cancel all orders |
//!
//! ### Positions
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/positions` | List all positions |
//! | GET | `/api/v1/positions/{symbol}` | Get position |
//!
//! ### Executions
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `/api/v1/executions` | List executions |
//! | GET | `/api/v1/executions/{execution_id}` | Get execution |
//!
//! ### Admin (Orderbook Persistence)
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | POST | `/api/v1/admin/snapshot` | Create orderbook snapshot |
//! | GET | `/api/v1/admin/snapshots` | List snapshots |
//! | GET | `/api/v1/admin/snapshots/{id}` | Get snapshot |
//! | POST | `/api/v1/admin/snapshots/{id}/restore` | Restore snapshot |
//!
//! ### WebSocket
//!
//! | Endpoint | Description |
//! |----------|-------------|
//! | `/ws` | WebSocket connection for real-time updates |
//!
//! WebSocket channels:
//! - `orderbook:{symbol}` - Orderbook updates
//! - `trades:{symbol}` - Trade executions
//! - `quotes:{symbol}` - Quote updates
//!
//! ## Example Usage
//!
//! ### Starting the Server
//!
//! ```bash
//! # Development mode
//! cargo run
//!
//! # With custom host/port
//! HOST=127.0.0.1 PORT=3000 cargo run
//!
//! # With configuration file
//! cargo run -- --config config.toml
//!
//! # Release build
//! cargo build --release
//! ./target/release/option-chain-orderbook-backend
//! ```
//!
//! ### API Requests
//!
//! ```bash
//! # Create an underlying
//! curl -X POST http://localhost:8080/api/v1/underlyings/BTC
//!
//! # Create an expiration (YYYYMMDD format)
//! curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329
//!
//! # Create a strike
//! curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000
//!
//! # Add a buy order to the call option
//! curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/orders \
//!   -H "Content-Type: application/json" \
//!   -d '{"side": "buy", "price": 100, "quantity": 10}'
//!
//! # Submit a market order
//! curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/orders/market \
//!   -H "Content-Type: application/json" \
//!   -d '{"side": "buy", "quantity": 10}'
//!
//! # Get the call option quote
//! curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/quote
//!
//! # Get option greeks
//! curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/greeks
//!
//! # Get orderbook metrics
//! curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/metrics
//!
//! # Get volatility surface
//! curl http://localhost:8080/api/v1/underlyings/BTC/volatility-surface
//!
//! # List positions
//! curl http://localhost:8080/api/v1/positions
//!
//! # List executions
//! curl http://localhost:8080/api/v1/executions
//!
//! # Create orderbook snapshot
//! curl -X POST http://localhost:8080/api/v1/admin/snapshot
//!
//! # Get global statistics
//! curl http://localhost:8080/api/v1/stats
//! ```
//!
//! ### Market Maker Controls
//!
//! ```bash
//! # Get current control status
//! curl http://localhost:8080/api/v1/controls
//!
//! # Activate kill switch (disable all quoting)
//! curl -X POST http://localhost:8080/api/v1/controls/kill-switch
//!
//! # Enable quoting
//! curl -X POST http://localhost:8080/api/v1/controls/enable
//!
//! # Update parameters
//! curl -X POST http://localhost:8080/api/v1/controls/parameters \
//!   -H "Content-Type: application/json" \
//!   -d '{"spreadMultiplier": 1.5, "sizeScalar": 200, "directionalSkew": 0.05}'
//!
//! # Insert underlying price
//! curl -X POST http://localhost:8080/api/v1/prices \
//!   -H "Content-Type: application/json" \
//!   -d '{"symbol": "BTC", "price": 50000.0}'
//! ```
//!
//! ## Swagger UI
//!
//! Once the server is running, access the interactive API documentation at:
//!
//! ```text
//! http://localhost:8080/swagger-ui/
//! ```
//!
//! ## Client Library
//!
//! A Rust client library is available in `crates/orderbook-client`:
//!
//! ```rust,ignore
//! use orderbook_client::{OrderbookClient, ClientConfig};
//!
//! let client = OrderbookClient::new(ClientConfig::default())?;
//!
//! // Health check
//! let health = client.health_check().await?;
//!
//! // Create underlying
//! let underlying = client.create_underlying("BTC").await?;
//!
//! // Add order
//! let path = OptionPath::call("BTC", "20240329", 50000);
//! let order = client.add_order(&path, &AddOrderRequest {
//!     side: OrderSide::Buy,
//!     price: 100,
//!     quantity: 10,
//! }).await?;
//!
//! // Create snapshot
//! let snapshot = client.create_snapshot().await?;
//! ```
//!
//! ## Dependencies
//!
//! - **axum** (0.8): Async web framework
//! - **tower-http** (0.6): HTTP middleware (CORS, tracing, compression)
//! - **option-chain-orderbook** (0.3): Core orderbook library
//! - **orderbook-rs** (0.5): Low-level orderbook implementation
//! - **optionstratlib** (0.14): Options pricing and greeks
//! - **utoipa** (5.4): OpenAPI documentation generation
//! - **utoipa-swagger-ui** (9.0): Swagger UI integration
//! - **tokio** (1.49): Async runtime
//! - **sqlx** (0.8): Database connectivity (PostgreSQL)
//! - **dashmap** (6.1): Concurrent hash maps
//! - **serde** (1.0): Serialization/deserialization
//! - **tracing** (0.1): Structured logging

pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod market_maker;
pub mod models;
pub mod ohlc;
pub mod simulation;
pub mod state;

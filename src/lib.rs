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
//! - **OpenAPI Documentation**: Auto-generated Swagger UI for API exploration
//!   and testing at `/swagger-ui/`.
//!
//! - **CORS Support**: Cross-origin resource sharing enabled for frontend integration.
//!
//! - **Structured Logging**: Request tracing with `tower-http` for debugging
//!   and monitoring.
//!
//! - **Thread-Safe State**: Shared application state using `Arc` for concurrent
//!   request handling.
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
//! | [`api`] | Route handlers and router configuration |
//! | [`error`] | API error types with `IntoResponse` implementation |
//! | [`models`] | Request/response DTOs with OpenAPI schemas |
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
//! | GET | `.../strikes/{strike}/options/{style}` | Get option book |
//! | POST | `.../strikes/{strike}/options/{style}/orders` | Add order |
//! | DELETE | `.../strikes/{strike}/options/{style}/orders/{id}` | Cancel order |
//! | GET | `.../strikes/{strike}/options/{style}/quote` | Get quote |
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
//! # Get the call option quote
//! curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/quote
//!
//! # Get global statistics
//! curl http://localhost:8080/api/v1/stats
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
//! ## Dependencies
//!
//! - **axum** (0.8): Async web framework
//! - **tower-http** (0.6): HTTP middleware (CORS, tracing, compression)
//! - **option-chain-orderbook**: Core orderbook library
//! - **utoipa** (5.3): OpenAPI documentation generation
//! - **utoipa-swagger-ui** (8.1): Swagger UI integration
//! - **tokio** (1.42): Async runtime
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

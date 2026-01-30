[![Dual License](https://img.shields.io/badge/license-MIT-blue)](./LICENSE)
[![Stars](https://img.shields.io/github/stars/joaquinbejar/Option-Chain-OrderBook-Backend.svg)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/stargazers)
[![Issues](https://img.shields.io/github/issues/joaquinbejar/Option-Chain-OrderBook-Backend.svg)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/issues)
[![PRs](https://img.shields.io/github/issues-pr/joaquinbejar/Option-Chain-OrderBook-Backend.svg)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/pulls)

[![Build Status](https://img.shields.io/github/workflow/status/joaquinbejar/Option-Chain-OrderBook-Backend/CI)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/actions)
[![Coverage](https://img.shields.io/codecov/c/github/joaquinbejar/Option-Chain-OrderBook-Backend)](https://codecov.io/gh/joaquinbejar/Option-Chain-OrderBook-Backend)
[![Dependencies](https://img.shields.io/librariesio/github/joaquinbejar/Option-Chain-OrderBook-Backend)](https://libraries.io/github/joaquinbejar/Option-Chain-OrderBook-Backend)



## Option Chain OrderBook Backend - REST API Server

A high-performance REST API backend for interacting with the
[Option Chain OrderBook](https://crates.io/crates/option-chain-orderbook) library.
Built with [Axum](https://crates.io/crates/axum) for async HTTP handling and
provides OpenAPI/Swagger documentation via [utoipa](https://crates.io/crates/utoipa).

### Key Features

- **RESTful API**: Full CRUD operations for the option chain orderbook hierarchy.

- **Hierarchical Access**: Navigate from underlying assets down to individual
  option order books through a clean URL structure.

- **Real-time WebSocket**: Subscribe to orderbook updates, trades, and market data.

- **Market Making Engine**: Built-in market maker with configurable spread,
  size, and skew parameters.

- **Position Tracking**: Track positions and P&L across all instruments.

- **Execution Reports**: Complete audit trail of all trade executions.

- **Orderbook Persistence**: Snapshot and restore orderbook state.

- **Rate Limiting**: Configurable per-key rate limiting for API access.

- **API Key Authentication**: Secure API access with permission-based keys.

- **OpenAPI Documentation**: Auto-generated Swagger UI for API exploration
  and testing at `/swagger-ui/`.

- **CORS Support**: Cross-origin resource sharing enabled for frontend integration.

- **Structured Logging**: Request tracing with `tower-http` for debugging
  and monitoring.

### Architecture

The API follows the hierarchical structure of the Option Chain OrderBook library:

```
/api/v1/underlyings                                    → UnderlyingOrderBookManager
  └── /api/v1/underlyings/{underlying}                 → UnderlyingOrderBook
        └── /expirations                               → ExpirationOrderBookManager
              └── /expirations/{expiration}            → ExpirationOrderBook
                    └── /strikes                       → StrikeOrderBookManager
                          └── /strikes/{strike}        → StrikeOrderBook
                                └── /options/{style}   → OptionOrderBook
```

### Module Structure

| Module | Description |
|--------|-------------|
| [`api`] | Route handlers, WebSocket, and router configuration |
| [`auth`] | API key authentication and rate limiting |
| [`config`] | Server and market maker configuration |
| [`db`] | Database connection pool and schema |
| [`error`] | API error types with `IntoResponse` implementation |
| [`market_maker`] | Market making engine with pricing and quoting |
| [`models`] | Request/response DTOs with OpenAPI schemas |
| [`ohlc`] | OHLC candlestick aggregation |
| [`simulation`] | Price simulation for testing |
| [`state`] | Application state management |

### API Endpoints

#### Health & Statistics

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| GET | `/api/v1/stats` | Global statistics |

#### Authentication

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/auth/keys` | Create API key |
| GET | `/api/v1/auth/keys` | List API keys |
| DELETE | `/api/v1/auth/keys/{key_id}` | Delete API key |

#### Controls (Market Maker)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/controls` | Get system control status |
| POST | `/api/v1/controls/kill-switch` | Disable all quoting |
| POST | `/api/v1/controls/enable` | Enable quoting |
| POST | `/api/v1/controls/parameters` | Update spread/size/skew |
| GET | `/api/v1/controls/instruments` | List instruments |
| POST | `/api/v1/controls/instrument/{symbol}/toggle` | Toggle instrument |

#### Prices

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/prices` | Insert underlying price |
| GET | `/api/v1/prices` | Get all prices |
| GET | `/api/v1/prices/{symbol}` | Get latest price |

#### Underlyings

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/underlyings` | List all underlyings |
| POST | `/api/v1/underlyings/{underlying}` | Create or get underlying |
| GET | `/api/v1/underlyings/{underlying}` | Get underlying details |
| DELETE | `/api/v1/underlyings/{underlying}` | Delete underlying |

#### Expirations

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/underlyings/{underlying}/expirations` | List expirations |
| POST | `/api/v1/underlyings/{underlying}/expirations/{exp}` | Create expiration |
| GET | `/api/v1/underlyings/{underlying}/expirations/{exp}` | Get expiration |

#### Volatility Surface

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/underlyings/{underlying}/volatility-surface` | Get IV surface |

#### Option Chain

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `.../expirations/{exp}/chain` | Get option chain matrix |

#### Strikes

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `.../expirations/{exp}/strikes` | List strikes |
| POST | `.../expirations/{exp}/strikes/{strike}` | Create strike |
| GET | `.../expirations/{exp}/strikes/{strike}` | Get strike details |

#### Options

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `.../options/{style}` | Get option book |
| POST | `.../options/{style}/orders` | Add limit order |
| POST | `.../options/{style}/orders/market` | Submit market order |
| DELETE | `.../options/{style}/orders/{id}` | Cancel order |
| PATCH | `.../options/{style}/orders/{id}` | Modify order |
| GET | `.../options/{style}/quote` | Get quote |
| GET | `.../options/{style}/greeks` | Get option greeks |
| GET | `.../options/{style}/snapshot` | Get enriched snapshot |
| GET | `.../options/{style}/last-trade` | Get last trade |
| GET | `.../options/{style}/ohlc` | Get OHLC bars |
| GET | `.../options/{style}/metrics` | Get orderbook metrics |

#### Orders

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/orders` | List all orders |
| GET | `/api/v1/orders/{order_id}` | Get order status |
| POST | `/api/v1/orders/bulk` | Bulk submit orders |
| DELETE | `/api/v1/orders/bulk` | Bulk cancel orders |
| DELETE | `/api/v1/orders/cancel-all` | Cancel all orders |

#### Positions

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/positions` | List all positions |
| GET | `/api/v1/positions/{symbol}` | Get position |

#### Executions

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/executions` | List executions |
| GET | `/api/v1/executions/{execution_id}` | Get execution |

#### Admin (Orderbook Persistence)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/admin/snapshot` | Create orderbook snapshot |
| GET | `/api/v1/admin/snapshots` | List snapshots |
| GET | `/api/v1/admin/snapshots/{id}` | Get snapshot |
| POST | `/api/v1/admin/snapshots/{id}/restore` | Restore snapshot |

#### WebSocket

| Endpoint | Description |
|----------|-------------|
| `/ws` | WebSocket connection for real-time updates |

WebSocket channels:
- `orderbook:{symbol}` - Orderbook updates
- `trades:{symbol}` - Trade executions
- `quotes:{symbol}` - Quote updates

### Example Usage

#### Starting the Server

```bash
# Development mode
cargo run

# With custom host/port
HOST=127.0.0.1 PORT=3000 cargo run

# With configuration file
cargo run -- --config config.toml

# Release build
cargo build --release
./target/release/option-chain-orderbook-backend
```

#### API Requests

```bash
# Create an underlying
curl -X POST http://localhost:8080/api/v1/underlyings/BTC

# Create an expiration (YYYYMMDD format)
curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329

# Create a strike
curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000

# Add a buy order to the call option
curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/orders \
  -H "Content-Type: application/json" \
  -d '{"side": "buy", "price": 100, "quantity": 10}'

# Submit a market order
curl -X POST http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/orders/market \
  -H "Content-Type: application/json" \
  -d '{"side": "buy", "quantity": 10}'

# Get the call option quote
curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/quote

# Get option greeks
curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/greeks

# Get orderbook metrics
curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/metrics

# Get volatility surface
curl http://localhost:8080/api/v1/underlyings/BTC/volatility-surface

# List positions
curl http://localhost:8080/api/v1/positions

# List executions
curl http://localhost:8080/api/v1/executions

# Create orderbook snapshot
curl -X POST http://localhost:8080/api/v1/admin/snapshot

# Get global statistics
curl http://localhost:8080/api/v1/stats
```

#### Market Maker Controls

```bash
# Get current control status
curl http://localhost:8080/api/v1/controls

# Activate kill switch (disable all quoting)
curl -X POST http://localhost:8080/api/v1/controls/kill-switch

# Enable quoting
curl -X POST http://localhost:8080/api/v1/controls/enable

# Update parameters
curl -X POST http://localhost:8080/api/v1/controls/parameters \
  -H "Content-Type: application/json" \
  -d '{"spreadMultiplier": 1.5, "sizeScalar": 200, "directionalSkew": 0.05}'

# Insert underlying price
curl -X POST http://localhost:8080/api/v1/prices \
  -H "Content-Type: application/json" \
  -d '{"symbol": "BTC", "price": 50000.0}'
```

### Swagger UI

Once the server is running, access the interactive API documentation at:

```
http://localhost:8080/swagger-ui/
```

### Client Library

A Rust client library is available in `crates/orderbook-client`:

```rust
use orderbook_client::{OrderbookClient, ClientConfig};

let client = OrderbookClient::new(ClientConfig::default())?;

// Health check
let health = client.health_check().await?;

// Create underlying
let underlying = client.create_underlying("BTC").await?;

// Add order
let path = OptionPath::call("BTC", "20240329", 50000);
let order = client.add_order(&path, &AddOrderRequest {
    side: OrderSide::Buy,
    price: 100,
    quantity: 10,
}).await?;

// Create snapshot
let snapshot = client.create_snapshot().await?;
```

### Dependencies

- **axum** (0.8): Async web framework
- **tower-http** (0.6): HTTP middleware (CORS, tracing, compression)
- **option-chain-orderbook** (0.3): Core orderbook library
- **orderbook-rs** (0.5): Low-level orderbook implementation
- **optionstratlib** (0.14): Options pricing and greeks
- **utoipa** (5.4): OpenAPI documentation generation
- **utoipa-swagger-ui** (9.0): Swagger UI integration
- **tokio** (1.49): Async runtime
- **sqlx** (0.8): Database connectivity (PostgreSQL)
- **dashmap** (6.1): Concurrent hash maps
- **serde** (1.0): Serialization/deserialization
- **tracing** (0.1): Structured logging


## 🛠 Makefile Commands

This project includes a `Makefile` with common tasks to simplify development. Here's a list of useful commands:

### 🔧 Build & Run

```sh
make build         # Compile the project
make release       # Build in release mode
make run           # Run the main binary
```

### 🧪 Test & Quality

```sh
make test          # Run all tests
make fmt           # Format code
make fmt-check     # Check formatting without applying
make lint          # Run clippy with warnings as errors
make lint-fix      # Auto-fix lint issues
make fix           # Auto-fix Rust compiler suggestions
make check         # Run fmt-check + lint + test
```

### 📦 Packaging & Docs

```sh
make doc           # Check for missing docs via clippy
make doc-open      # Build and open Rust documentation
make create-doc    # Generate internal docs
make readme        # Regenerate README using cargo-readme
make publish       # Prepare and publish crate to crates.io
```

### 📈 Coverage & Benchmarks

```sh
make coverage            # Generate code coverage report (XML)
make coverage-html       # Generate HTML coverage report
make open-coverage       # Open HTML report
make bench               # Run benchmarks using Criterion
make bench-show          # Open benchmark report
make bench-save          # Save benchmark history snapshot
make bench-compare       # Compare benchmark runs
make bench-json          # Output benchmarks in JSON
make bench-clean         # Remove benchmark data
```

### 🧪 Git & Workflow Helpers

```sh
make git-log             # Show commits on current branch vs main
make check-spanish       # Check for Spanish words in code
make zip                 # Create zip without target/ and temp files
make tree                # Visualize project tree (excludes common clutter)
```

### 🤖 GitHub Actions (via act)

```sh
make workflow-build      # Simulate build workflow
make workflow-lint       # Simulate lint workflow
make workflow-test       # Simulate test workflow
make workflow-coverage   # Simulate coverage workflow
make workflow            # Run all workflows
```

ℹ️ Requires act for local workflow simulation and cargo-tarpaulin for coverage.

## Contribution and Contact

We welcome contributions to this project! If you would like to contribute, please follow these steps:

1. Fork the repository.
2. Create a new branch for your feature or bug fix.
3. Make your changes and ensure that the project still builds and all tests pass.
4. Commit your changes and push your branch to your forked repository.
5. Submit a pull request to the main repository.

If you have any questions, issues, or would like to provide feedback, please feel free to contact the project
maintainer:

### **Contact Information**
- **Author**: Joaquín Béjar García
- **Email**: jb@taunais.com
- **Telegram**: [@joaquin_bejar](https://t.me/joaquin_bejar)
- **Repository**: <https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend>


We appreciate your interest and look forward to your contributions!

**License**: MIT

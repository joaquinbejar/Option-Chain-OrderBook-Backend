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

- **Last Trade Data**: Retrieve the most recent trade information for any option contract.

- **Real-time WebSocket Events**: Live trade notifications broadcast to connected clients.

- **Hierarchical Access**: Navigate from underlying assets down to individual
  option order books through a clean URL structure.

- **OpenAPI Documentation**: Auto-generated Swagger UI for API exploration
  and testing at `/swagger-ui/`.

- **CORS Support**: Cross-origin resource sharing enabled for frontend integration.

- **Structured Logging**: Request tracing with `tower-http` for debugging
  and monitoring.

- **Thread-Safe State**: Shared application state using `Arc` for concurrent
  request handling.

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
| [`api`] | Route handlers and router configuration |
| [`error`] | API error types with `IntoResponse` implementation |
| [`models`] | Request/response DTOs with OpenAPI schemas |
| [`state`] | Application state management |

### API Endpoints

#### Health & Statistics

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| GET | `/api/v1/stats` | Global statistics |

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

#### Strikes

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `.../expirations/{exp}/strikes` | List strikes |
| POST | `.../expirations/{exp}/strikes/{strike}` | Create strike |
| GET | `.../expirations/{exp}/strikes/{strike}` | Get strike details |

#### Options

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `.../strikes/{strike}/options/{style}` | Get option book |
| POST | `.../strikes/{strike}/options/{style}/orders` | Add order |
| DELETE | `.../strikes/{strike}/options/{style}/orders/{id}` | Cancel order |
| GET | `.../strikes/{strike}/options/{style}/quote` | Get quote |
| GET | `.../strikes/{strike}/options/{style}/last-trade` | Get last trade information |

### Example Usage

#### Starting the Server

```bash
# Development mode
cargo run

# With custom host/port
HOST=127.0.0.1 PORT=3000 cargo run

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

# Get call option quote
curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/quote

# Get last trade information
curl http://localhost:8080/api/v1/underlyings/BTC/expirations/20240329/strikes/50000/options/call/last-trade

# Get global statistics
curl http://localhost:8080/api/v1/stats
```

### Swagger UI

Once the server is running, access the interactive API documentation at:

```
http://localhost:8080/swagger-ui/
```

### Dependencies

- **axum** (0.8): Async web framework
- **tower-http** (0.6): HTTP middleware (CORS, tracing, compression)
- **option-chain-orderbook**: Core orderbook library
- **utoipa** (5.3): OpenAPI documentation generation
- **utoipa-swagger-ui** (8.1): Swagger UI integration
- **tokio** (1.42): Async runtime
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

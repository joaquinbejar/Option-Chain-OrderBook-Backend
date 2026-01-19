# orderbook-tests

Integration tests for the Option Chain OrderBook API.

## Prerequisites

The API server must be running before executing these tests.

## Configuration

Set the `API_BASE_URL` environment variable to point to your API server:

```bash
export API_BASE_URL=http://localhost:8080
```

Default: `http://localhost:8080`

## Running Tests

### Start the API server first

```bash
# From the project root
cargo run --release
```

### Run integration tests

```bash
# Run all integration tests
cargo test -p orderbook-tests

# Run specific test file
cargo test -p orderbook-tests --test health
cargo test -p orderbook-tests --test orderbook
cargo test -p orderbook-tests --test websocket
cargo test -p orderbook-tests --test market_maker

# Run with output
cargo test -p orderbook-tests -- --nocapture
```

### Using Docker

```bash
# Start API and run tests
docker compose --profile test up
```

## Test Coverage

### Health Tests (`tests/health.rs`)
- Health check endpoint
- Global statistics endpoint

### Orderbook Tests (`tests/orderbook.rs`)
- Create and list underlyings
- Create expirations and strikes
- Add and cancel orders
- Get option quotes
- Market order execution
- Market order with no liquidity
- Put option operations

### WebSocket Tests (`tests/websocket.rs`)
- WebSocket connection
- Subscribe/unsubscribe commands
- Heartbeat handling

### Market Maker Tests (`tests/market_maker.rs`)
- Get controls
- Kill switch and enable
- Update parameters
- List instruments
- Toggle instrument
- Insert and get prices
- Get all prices
- Price not found error

## Test Isolation

Each test uses unique symbols generated with timestamps to avoid conflicts when running tests in parallel.

## License

MIT

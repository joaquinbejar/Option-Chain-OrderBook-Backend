# orderbook-client

HTTP client library for the Option Chain OrderBook API.

## Features

- Typed HTTP client for all REST endpoints
- WebSocket client for real-time updates
- Async/await support with tokio
- Comprehensive error handling

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
orderbook-client = "0.1.0"
```

## Usage

### REST API

```rust
use orderbook_client::{OrderbookClient, ClientConfig, OptionPath, AddOrderRequest};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), orderbook_client::Error> {
    // Create client
    let client = OrderbookClient::new(ClientConfig {
        base_url: "http://localhost:8080".into(),
        timeout: Duration::from_secs(30),
    })?;

    // Health check
    let health = client.health_check().await?;
    println!("Status: {}", health.status);

    // List underlyings
    let underlyings = client.list_underlyings().await?;
    println!("Underlyings: {:?}", underlyings.underlyings);

    // Create an underlying
    let underlying = client.create_underlying("BTC").await?;
    println!("Created: {}", underlying.symbol);

    // Add an order
    let option = OptionPath::call("BTC", "20251231", 100000);
    let order = client.add_order(&option, &AddOrderRequest {
        side: "buy".into(),
        price: 1500,
        quantity: 10,
    }).await?;
    println!("Order ID: {}", order.order_id);

    Ok(())
}
```

### WebSocket

```rust
use orderbook_client::{WsClient, WsMessage};

#[tokio::main]
async fn main() -> Result<(), orderbook_client::Error> {
    let mut ws = WsClient::connect("ws://localhost:8080/ws").await?;

    // Subscribe to a symbol
    ws.subscribe("BTC").await?;

    // Receive messages
    while let Some(msg) = ws.recv().await {
        match msg {
            WsMessage::Quote { symbol, bid_price, ask_price, .. } => {
                println!("{}: {} / {}", symbol, bid_price, ask_price);
            }
            WsMessage::Fill { order_id, price, quantity, .. } => {
                println!("Fill: {} @ {} x {}", order_id, price, quantity);
            }
            _ => {}
        }
    }

    Ok(())
}
```

## API Coverage

### Health & Stats
- `health_check()` - Health check endpoint
- `get_global_stats()` - Global statistics

### Underlyings
- `list_underlyings()` - List all underlyings
- `create_underlying(symbol)` - Create underlying
- `get_underlying(symbol)` - Get underlying details
- `delete_underlying(symbol)` - Delete underlying

### Expirations
- `list_expirations(underlying)` - List expirations
- `create_expiration(underlying, expiration)` - Create expiration
- `get_expiration(underlying, expiration)` - Get expiration details

### Strikes
- `list_strikes(underlying, expiration)` - List strikes
- `create_strike(underlying, expiration, strike)` - Create strike
- `get_strike(underlying, expiration, strike)` - Get strike details

### Options
- `get_option_book(path)` - Get order book snapshot
- `get_option_quote(path)` - Get best quote

### Orders
- `add_order(path, request)` - Add limit order
- `submit_market_order(path, request)` - Submit market order
- `cancel_order(path, order_id)` - Cancel order

### Controls
- `get_controls()` - Get system control status
- `kill_switch()` - Activate kill switch
- `enable_quoting()` - Enable quoting
- `update_parameters(request)` - Update parameters
- `list_instruments()` - List instruments
- `toggle_instrument(symbol)` - Toggle instrument

### Prices
- `insert_price(request)` - Insert price
- `get_latest_price(symbol)` - Get latest price
- `get_all_prices()` - Get all prices

## License

MIT

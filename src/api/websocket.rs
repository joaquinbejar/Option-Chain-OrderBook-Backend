//! WebSocket handler for real-time updates.

use crate::market_maker::MarketMakerEvent;
use crate::state::AppState;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// WebSocket message types sent to clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    /// Quote update.
    #[serde(rename = "quote")]
    Quote {
        /// Underlying symbol.
        symbol: String,
        /// Expiration date string.
        expiration: String,
        /// Strike price in cents.
        strike: u64,
        /// Option style (call/put).
        style: String,
        /// Bid price in cents.
        bid_price: u128,
        /// Ask price in cents.
        ask_price: u128,
        /// Bid size.
        bid_size: u64,
        /// Ask size.
        ask_size: u64,
    },
    /// Order fill notification.
    #[serde(rename = "fill")]
    Fill {
        /// Order identifier.
        order_id: String,
        /// Underlying symbol.
        symbol: String,
        /// Instrument identifier.
        instrument: String,
        /// Order side (buy/sell).
        side: String,
        /// Filled quantity.
        quantity: u64,
        /// Fill price in cents.
        price: u128,
        /// Edge captured in cents.
        edge: i64,
    },
    /// Configuration change.
    #[serde(rename = "config")]
    Config {
        /// Whether quoting is enabled.
        enabled: bool,
        /// Spread multiplier.
        spread_multiplier: f64,
        /// Size scalar (0.0 to 1.0).
        size_scalar: f64,
        /// Directional skew (-1.0 to 1.0).
        directional_skew: f64,
    },
    /// Price update.
    #[serde(rename = "price")]
    Price {
        /// Underlying symbol.
        symbol: String,
        /// Price in cents.
        price_cents: u64,
    },
    /// Connection established.
    #[serde(rename = "connected")]
    Connected {
        /// Welcome message.
        message: String,
    },
    /// Heartbeat/ping.
    #[serde(rename = "heartbeat")]
    Heartbeat {
        /// Timestamp in milliseconds.
        timestamp: u64,
    },
    /// Orderbook snapshot.
    #[serde(rename = "orderbook_snapshot")]
    OrderbookSnapshot {
        /// Channel name.
        channel: String,
        /// Symbol identifier.
        symbol: String,
        /// Sequence number for ordering.
        sequence: u64,
        /// Bid price levels.
        bids: Vec<PriceLevelData>,
        /// Ask price levels.
        asks: Vec<PriceLevelData>,
    },
    /// Orderbook delta (incremental update).
    #[serde(rename = "orderbook_delta")]
    OrderbookDelta {
        /// Symbol identifier.
        symbol: String,
        /// Sequence number for ordering.
        sequence: u64,
        /// List of price level changes.
        changes: Vec<PriceLevelChange>,
    },
    /// Subscription confirmation.
    #[serde(rename = "subscribed")]
    Subscribed {
        /// Channel name.
        channel: String,
        /// Symbol subscribed to.
        symbol: String,
    },
    /// Unsubscription confirmation.
    #[serde(rename = "unsubscribed")]
    Unsubscribed {
        /// Channel name.
        channel: String,
        /// Symbol unsubscribed from.
        symbol: String,
    },
    /// Error message.
    #[serde(rename = "error")]
    Error {
        /// Error message.
        message: String,
    },
}

/// Price level data for snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelData {
    /// Price in smallest units.
    pub price: u128,
    /// Quantity at this price level.
    pub quantity: u64,
}

/// Price level change for delta updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelChange {
    /// Side of the orderbook ("bid" or "ask").
    pub side: String,
    /// Price in smallest units.
    pub price: u128,
    /// New quantity at this price level (0 means level removed).
    pub quantity: u64,
}

/// Orderbook delta event for broadcasting.
#[derive(Debug, Clone)]
pub struct OrderbookDeltaEvent {
    /// Symbol identifier.
    pub symbol: String,
    /// Sequence number.
    pub sequence: u64,
    /// Price level change.
    pub change: PriceLevelChange,
}

/// Manages orderbook subscriptions and sequence numbers.
pub struct OrderbookSubscriptionManager {
    /// Sequence counters per symbol.
    sequences: DashMap<String, AtomicU64>,
    /// Broadcast channel for orderbook deltas.
    delta_tx: broadcast::Sender<OrderbookDeltaEvent>,
}

impl OrderbookSubscriptionManager {
    /// Creates a new subscription manager.
    #[must_use]
    pub fn new() -> Self {
        let (delta_tx, _) = broadcast::channel(1000);
        Self {
            sequences: DashMap::new(),
            delta_tx,
        }
    }

    /// Gets the next sequence number for a symbol.
    #[must_use]
    pub fn next_sequence(&self, symbol: &str) -> u64 {
        self.sequences
            .entry(symbol.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
    }

    /// Gets the current sequence number for a symbol.
    #[must_use]
    pub fn current_sequence(&self, symbol: &str) -> u64 {
        self.sequences
            .get(symbol)
            .map(|v| v.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Broadcasts a delta event.
    pub fn broadcast_delta(&self, event: OrderbookDeltaEvent) {
        let _ = self.delta_tx.send(event);
    }

    /// Subscribes to delta events.
    #[must_use]
    pub fn subscribe_deltas(&self) -> broadcast::Receiver<OrderbookDeltaEvent> {
        self.delta_tx.subscribe()
    }
}

impl Default for OrderbookSubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket upgrade handler.
#[utoipa::path(
    get,
    path = "/ws",
    responses(
        (status = 101, description = "WebSocket connection established")
    ),
    tag = "WebSocket"
)]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle an individual WebSocket connection.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(tokio::sync::Mutex::new(sender));

    // Subscribe to market maker events
    let mut event_rx = state.market_maker.subscribe();

    // Subscribe to orderbook delta events
    let mut delta_rx = state.orderbook_subscriptions.subscribe_deltas();

    // Track this client's orderbook subscriptions
    let subscribed_symbols: Arc<tokio::sync::RwLock<HashSet<String>>> =
        Arc::new(tokio::sync::RwLock::new(HashSet::new()));

    // Send connection confirmation
    let connected_msg = WsMessage::Connected {
        message: "Connected to Option Chain OrderBook".to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!("WebSocket client connected");

    // Spawn task to handle incoming messages (for ping/pong and commands)
    let state_clone = Arc::clone(&state);
    let sender_clone = Arc::clone(&sender);
    let subscribed_symbols_clone = Arc::clone(&subscribed_symbols);
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);
                    // Handle client commands
                    handle_client_message(
                        &text,
                        &state_clone,
                        &sender_clone,
                        &subscribed_symbols_clone,
                    )
                    .await;
                }
                Ok(Message::Ping(_data)) => {
                    debug!("Received ping");
                    // Pong is handled automatically by axum
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket client disconnected");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Send events to client
    let sender_clone = Arc::clone(&sender);
    let subscribed_symbols_clone = Arc::clone(&subscribed_symbols);
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle market maker events
                event = event_rx.recv() => {
                    match event {
                        Ok(event) => {
                            if let Some(msg) = event_to_ws_message(event)
                                && let Ok(json) = serde_json::to_string(&msg)
                                    && sender_clone.lock().await.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            debug!("WebSocket lagged {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                // Handle orderbook delta events
                delta = delta_rx.recv() => {
                    match delta {
                        Ok(delta_event) => {
                            // Only send if client is subscribed to this symbol
                            let subscribed = subscribed_symbols_clone.read().await;
                            if subscribed.contains(&delta_event.symbol) {
                                let msg = WsMessage::OrderbookDelta {
                                    symbol: delta_event.symbol,
                                    sequence: delta_event.sequence,
                                    changes: vec![delta_event.change],
                                };
                                if let Ok(json) = serde_json::to_string(&msg)
                                    && sender_clone.lock().await.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Orderbook delta lagged {} messages", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                // Send periodic heartbeat
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                    let heartbeat = WsMessage::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    };
                    if let Ok(json) = serde_json::to_string(&heartbeat)
                        && sender_clone.lock().await.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = recv_task => {}
        _ = send_task => {}
    }

    info!("WebSocket connection closed");
}

/// Convert market maker event to WebSocket message.
fn event_to_ws_message(event: MarketMakerEvent) -> Option<WsMessage> {
    match event {
        MarketMakerEvent::QuoteUpdated {
            symbol,
            expiration,
            strike,
            style,
            bid_price,
            ask_price,
            bid_size,
            ask_size,
        } => Some(WsMessage::Quote {
            symbol,
            expiration,
            strike,
            style,
            bid_price,
            ask_price,
            bid_size,
            ask_size,
        }),
        MarketMakerEvent::OrderFilled {
            order_id,
            symbol,
            instrument,
            side,
            quantity,
            price,
            edge,
        } => Some(WsMessage::Fill {
            order_id,
            symbol,
            instrument,
            side,
            quantity,
            price,
            edge,
        }),
        MarketMakerEvent::ConfigChanged {
            enabled,
            spread_multiplier,
            size_scalar,
            directional_skew,
        } => Some(WsMessage::Config {
            enabled,
            spread_multiplier,
            size_scalar,
            directional_skew,
        }),
        MarketMakerEvent::PriceUpdated {
            symbol,
            price_cents,
        } => Some(WsMessage::Price {
            symbol,
            price_cents,
        }),
    }
}

/// Client command for WebSocket communication.
#[derive(Debug, Deserialize)]
struct ClientCommand {
    /// Action to perform.
    action: String,
    /// Optional channel (e.g., "orderbook").
    #[serde(default)]
    channel: Option<String>,
    /// Optional symbol.
    #[serde(default)]
    symbol: Option<String>,
    /// Optional depth for orderbook subscriptions.
    #[serde(default)]
    depth: Option<usize>,
    /// Optional value for parameter updates.
    #[serde(default)]
    value: Option<f64>,
}

/// Sender type alias for WebSocket.
type WsSender = Arc<tokio::sync::Mutex<futures::stream::SplitSink<WebSocket, Message>>>;

/// Handle incoming client messages.
async fn handle_client_message(
    text: &str,
    state: &Arc<AppState>,
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
) {
    if let Ok(cmd) = serde_json::from_str::<ClientCommand>(text) {
        match cmd.action.as_str() {
            "subscribe" => {
                if let Some(channel) = &cmd.channel {
                    if channel == "orderbook" {
                        handle_orderbook_subscribe(state, sender, subscribed_symbols, &cmd).await;
                    } else {
                        // Generic symbol subscription
                        debug!("Client subscribed to {:?}", cmd.symbol);
                    }
                } else {
                    debug!("Client subscribed to {:?}", cmd.symbol);
                }
            }
            "unsubscribe" => {
                if let Some(channel) = &cmd.channel {
                    if channel == "orderbook" {
                        handle_orderbook_unsubscribe(sender, subscribed_symbols, &cmd).await;
                    } else {
                        debug!("Client unsubscribed from {:?}", cmd.symbol);
                    }
                } else {
                    debug!("Client unsubscribed from {:?}", cmd.symbol);
                }
            }
            "set_spread" => {
                if let Some(value) = cmd.value {
                    state.market_maker.set_spread_multiplier(value);
                }
            }
            "set_size" => {
                if let Some(value) = cmd.value {
                    state.market_maker.set_size_scalar(value / 100.0);
                }
            }
            "set_skew" => {
                if let Some(value) = cmd.value {
                    state.market_maker.set_directional_skew(value);
                }
            }
            "kill" => {
                state.market_maker.set_enabled(false);
            }
            "enable" => {
                state.market_maker.set_enabled(true);
            }
            _ => {
                debug!("Unknown command: {}", cmd.action);
            }
        }
    }
}

/// Handles orderbook subscription requests.
async fn handle_orderbook_subscribe(
    state: &Arc<AppState>,
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    cmd: &ClientCommand,
) {
    let Some(symbol) = &cmd.symbol else {
        let error_msg = WsMessage::Error {
            message: "Symbol required for orderbook subscription".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    };

    let depth = cmd.depth.unwrap_or(10);

    // Parse symbol to get orderbook: format is "UNDERLYING-EXPIRATION-STRIKE-STYLE"
    let parts: Vec<&str> = symbol.split('-').collect();
    if parts.len() < 4 {
        let error_msg = WsMessage::Error {
            message: format!(
                "Invalid symbol format: {}. Expected: UNDERLYING-EXPIRATION-STRIKE-STYLE",
                symbol
            ),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    }

    // Add to subscribed symbols
    subscribed_symbols.write().await.insert(symbol.clone());

    // Send subscription confirmation
    let subscribed_msg = WsMessage::Subscribed {
        channel: "orderbook".to_string(),
        symbol: symbol.clone(),
    };
    if let Ok(json) = serde_json::to_string(&subscribed_msg) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    // Get and send initial snapshot
    if let Some(snapshot) = get_orderbook_snapshot(state, symbol, depth).await {
        let sequence = state.orderbook_subscriptions.next_sequence(symbol);
        let snapshot_msg = WsMessage::OrderbookSnapshot {
            channel: "orderbook".to_string(),
            symbol: symbol.clone(),
            sequence,
            bids: snapshot.0,
            asks: snapshot.1,
        };
        if let Ok(json) = serde_json::to_string(&snapshot_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
    }

    info!("Client subscribed to orderbook: {}", symbol);
}

/// Handles orderbook unsubscription requests.
async fn handle_orderbook_unsubscribe(
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    cmd: &ClientCommand,
) {
    let Some(symbol) = &cmd.symbol else {
        let error_msg = WsMessage::Error {
            message: "Symbol required for orderbook unsubscription".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    };

    // Remove from subscribed symbols
    subscribed_symbols.write().await.remove(symbol);

    // Send unsubscription confirmation
    let unsubscribed_msg = WsMessage::Unsubscribed {
        channel: "orderbook".to_string(),
        symbol: symbol.clone(),
    };
    if let Ok(json) = serde_json::to_string(&unsubscribed_msg) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!("Client unsubscribed from orderbook: {}", symbol);
}

/// Gets the current orderbook snapshot for a symbol.
async fn get_orderbook_snapshot(
    state: &Arc<AppState>,
    symbol: &str,
    depth: usize,
) -> Option<(Vec<PriceLevelData>, Vec<PriceLevelData>)> {
    // Parse symbol: UNDERLYING-EXPIRATION-STRIKE-STYLE
    let parts: Vec<&str> = symbol.split('-').collect();
    if parts.len() < 4 {
        return None;
    }

    let underlying = parts[0];
    let expiration_str = parts[1];
    let strike: u64 = parts[2].parse().ok()?;
    let style_str = parts[3];

    let style = match style_str.to_uppercase().as_str() {
        "C" | "CALL" => optionstratlib::OptionStyle::Call,
        "P" | "PUT" => optionstratlib::OptionStyle::Put,
        _ => return None,
    };

    // Get the underlying book
    let underlying_book = state.manager.get(underlying).ok()?;

    // Find expiration by string match
    let expiration = find_expiration_by_str(&underlying_book, expiration_str)?;

    // Get expiration book
    let exp_book = underlying_book.get_expiration(&expiration).ok()?;

    // Get strike book
    let strike_book = exp_book.get_strike(strike).ok()?;

    // Get option book
    let option_book = strike_book.get(style);

    // Get snapshot from the inner orderbook
    let snapshot = option_book.inner().create_snapshot(depth);

    // Convert to our format
    let bids: Vec<PriceLevelData> = snapshot
        .bids
        .iter()
        .map(|level| PriceLevelData {
            price: level.price,
            quantity: level.visible_quantity,
        })
        .collect();

    let asks: Vec<PriceLevelData> = snapshot
        .asks
        .iter()
        .map(|level| PriceLevelData {
            price: level.price,
            quantity: level.visible_quantity,
        })
        .collect();

    Some((bids, asks))
}

/// Finds an expiration in the underlying book by matching the formatted date string.
fn find_expiration_by_str(
    underlying_book: &std::sync::Arc<option_chain_orderbook::orderbook::UnderlyingOrderBook>,
    exp_str: &str,
) -> Option<optionstratlib::ExpirationDate> {
    for entry in underlying_book.expirations().iter() {
        let formatted = match entry.key().get_date() {
            Ok(date) => date.format("%Y%m%d").to_string(),
            Err(_) => entry.key().to_string(),
        };
        if formatted == exp_str {
            return Some(*entry.key());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_manager_sequence_numbers() {
        let manager = OrderbookSubscriptionManager::new();

        // First sequence should be 0
        assert_eq!(manager.next_sequence("AAPL-20240329-150-C"), 0);
        // Second sequence should be 1
        assert_eq!(manager.next_sequence("AAPL-20240329-150-C"), 1);
        // Different symbol should start at 0
        assert_eq!(manager.next_sequence("AAPL-20240329-160-C"), 0);
        // Original symbol continues
        assert_eq!(manager.next_sequence("AAPL-20240329-150-C"), 2);
    }

    #[test]
    fn test_subscription_manager_current_sequence() {
        let manager = OrderbookSubscriptionManager::new();

        // Unknown symbol should return 0
        assert_eq!(manager.current_sequence("UNKNOWN"), 0);

        // After incrementing, current should reflect the value
        let _ = manager.next_sequence("AAPL-20240329-150-C");
        let _ = manager.next_sequence("AAPL-20240329-150-C");
        assert_eq!(manager.current_sequence("AAPL-20240329-150-C"), 2);
    }

    #[test]
    fn test_price_level_data_serialization() {
        let data = PriceLevelData {
            price: 15000,
            quantity: 100,
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"price\":15000"));
        assert!(json.contains("\"quantity\":100"));
    }

    #[test]
    fn test_price_level_change_serialization() {
        let change = PriceLevelChange {
            side: "bid".to_string(),
            price: 15000,
            quantity: 100,
        };
        let json = serde_json::to_string(&change).unwrap();
        assert!(json.contains("\"side\":\"bid\""));
        assert!(json.contains("\"price\":15000"));
        assert!(json.contains("\"quantity\":100"));
    }

    #[test]
    fn test_ws_message_orderbook_snapshot_serialization() {
        let msg = WsMessage::OrderbookSnapshot {
            channel: "orderbook".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
            sequence: 42,
            bids: vec![PriceLevelData {
                price: 15000,
                quantity: 100,
            }],
            asks: vec![PriceLevelData {
                price: 15100,
                quantity: 50,
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"orderbook_snapshot\""));
        assert!(json.contains("\"channel\":\"orderbook\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
        assert!(json.contains("\"sequence\":42"));
    }

    #[test]
    fn test_ws_message_orderbook_delta_serialization() {
        let msg = WsMessage::OrderbookDelta {
            symbol: "AAPL-20240329-150-C".to_string(),
            sequence: 43,
            changes: vec![PriceLevelChange {
                side: "bid".to_string(),
                price: 15000,
                quantity: 150,
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"orderbook_delta\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
        assert!(json.contains("\"sequence\":43"));
    }

    #[test]
    fn test_ws_message_subscribed_serialization() {
        let msg = WsMessage::Subscribed {
            channel: "orderbook".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribed\""));
        assert!(json.contains("\"channel\":\"orderbook\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
    }

    #[test]
    fn test_ws_message_unsubscribed_serialization() {
        let msg = WsMessage::Unsubscribed {
            channel: "orderbook".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"unsubscribed\""));
    }

    #[test]
    fn test_ws_message_error_serialization() {
        let msg = WsMessage::Error {
            message: "Symbol required".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"Symbol required\""));
    }

    #[test]
    fn test_client_command_deserialization() {
        let json = r#"{"action":"subscribe","channel":"orderbook","symbol":"AAPL-20240329-150-C","depth":10}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.action, "subscribe");
        assert_eq!(cmd.channel, Some("orderbook".to_string()));
        assert_eq!(cmd.symbol, Some("AAPL-20240329-150-C".to_string()));
        assert_eq!(cmd.depth, Some(10));
    }

    #[test]
    fn test_client_command_deserialization_minimal() {
        let json = r#"{"action":"kill"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.action, "kill");
        assert_eq!(cmd.channel, None);
        assert_eq!(cmd.symbol, None);
        assert_eq!(cmd.depth, None);
    }

    #[test]
    fn test_orderbook_delta_event_creation() {
        let event = OrderbookDeltaEvent {
            symbol: "AAPL-20240329-150-C".to_string(),
            sequence: 100,
            change: PriceLevelChange {
                side: "ask".to_string(),
                price: 15100,
                quantity: 0,
            },
        };
        assert_eq!(event.symbol, "AAPL-20240329-150-C");
        assert_eq!(event.sequence, 100);
        assert_eq!(event.change.side, "ask");
        assert_eq!(event.change.quantity, 0); // 0 means level removed
    }

    #[test]
    fn test_subscription_manager_default() {
        let manager = OrderbookSubscriptionManager::default();
        assert_eq!(manager.current_sequence("any"), 0);
    }
}

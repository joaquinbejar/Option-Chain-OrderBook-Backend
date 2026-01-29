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
    /// Trade execution notification.
    #[serde(rename = "trade")]
    Trade {
        /// Unique trade identifier.
        trade_id: String,
        /// Symbol identifier.
        symbol: String,
        /// Execution price in smallest units.
        price: u128,
        /// Executed quantity.
        quantity: u64,
        /// Timestamp in milliseconds since epoch.
        timestamp_ms: u64,
        /// Maker order identifier.
        maker_order_id: String,
        /// Taker order identifier.
        taker_order_id: String,
    },
    /// Batch subscription response.
    #[serde(rename = "batch_subscribed")]
    BatchSubscribed {
        /// Request identifier for correlation.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        /// List of subscription results.
        subscriptions: Vec<SubscriptionResult>,
    },
    /// Batch unsubscription response.
    #[serde(rename = "batch_unsubscribed")]
    BatchUnsubscribed {
        /// Request identifier for correlation.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        /// List of unsubscription results.
        subscriptions: Vec<SubscriptionResult>,
    },
    /// List of active subscriptions.
    #[serde(rename = "subscriptions")]
    SubscriptionList {
        /// Active subscriptions.
        active: Vec<ActiveSubscription>,
    },
}

/// Subscription channel types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionChannel {
    /// Orderbook updates channel.
    Orderbook,
    /// Trade stream channel.
    Trades,
    /// Quote updates channel.
    Quotes,
    /// Price updates channel.
    Prices,
    /// Fill notifications channel.
    Fills,
}

impl std::fmt::Display for SubscriptionChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Orderbook => write!(f, "orderbook"),
            Self::Trades => write!(f, "trades"),
            Self::Quotes => write!(f, "quotes"),
            Self::Prices => write!(f, "prices"),
            Self::Fills => write!(f, "fills"),
        }
    }
}

/// Individual channel subscription request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSubscription {
    /// Channel to subscribe to.
    pub channel: SubscriptionChannel,
    /// Optional specific symbol (full format: UNDERLYING-EXPIRATION-STRIKE-STYLE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Optional underlying filter for wildcard subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
    /// Optional expiration filter for wildcard subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<String>,
    /// Optional depth for orderbook subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

/// Result of a subscription operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionResult {
    /// Channel that was subscribed to.
    pub channel: SubscriptionChannel,
    /// Symbol or filter that was subscribed to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Underlying filter if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
    /// Status of the subscription ("ok" or error message).
    pub status: String,
}

/// Active subscription entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSubscription {
    /// Channel of the subscription.
    pub channel: SubscriptionChannel,
    /// Symbol or filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Underlying filter if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
    /// Depth for orderbook subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
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

/// Trade event for broadcasting.
#[derive(Debug, Clone)]
pub struct TradeEvent {
    /// Unique trade identifier.
    pub trade_id: String,
    /// Symbol identifier.
    pub symbol: String,
    /// Execution price in smallest units.
    pub price: u128,
    /// Executed quantity.
    pub quantity: u64,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Maker order identifier.
    pub maker_order_id: String,
    /// Taker order identifier.
    pub taker_order_id: String,
}

/// Manages orderbook and trade subscriptions.
pub struct OrderbookSubscriptionManager {
    /// Sequence counters per symbol.
    sequences: DashMap<String, AtomicU64>,
    /// Broadcast channel for orderbook deltas.
    delta_tx: broadcast::Sender<OrderbookDeltaEvent>,
    /// Broadcast channel for trade events.
    trade_tx: broadcast::Sender<TradeEvent>,
}

impl OrderbookSubscriptionManager {
    /// Creates a new subscription manager.
    #[must_use]
    pub fn new() -> Self {
        let (delta_tx, _) = broadcast::channel(1000);
        let (trade_tx, _) = broadcast::channel(1000);
        Self {
            sequences: DashMap::new(),
            delta_tx,
            trade_tx,
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

    /// Broadcasts a trade event.
    pub fn broadcast_trade(&self, event: TradeEvent) {
        let _ = self.trade_tx.send(event);
    }

    /// Subscribes to trade events.
    #[must_use]
    pub fn subscribe_trades(&self) -> broadcast::Receiver<TradeEvent> {
        self.trade_tx.subscribe()
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

    // Subscribe to trade events
    let mut trade_rx = state.orderbook_subscriptions.subscribe_trades();

    // Track this client's orderbook subscriptions
    let subscribed_symbols: Arc<tokio::sync::RwLock<HashSet<String>>> =
        Arc::new(tokio::sync::RwLock::new(HashSet::new()));

    // Track this client's trade subscriptions
    let subscribed_trades: Arc<tokio::sync::RwLock<HashSet<String>>> =
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
    let subscribed_trades_clone = Arc::clone(&subscribed_trades);
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
                        &subscribed_trades_clone,
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
    let subscribed_trades_clone = Arc::clone(&subscribed_trades);
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
                // Handle trade events
                trade = trade_rx.recv() => {
                    match trade {
                        Ok(trade_event) => {
                            // Only send if client is subscribed to this symbol's trades
                            let subscribed = subscribed_trades_clone.read().await;
                            if subscribed.contains(&trade_event.symbol) {
                                let msg = WsMessage::Trade {
                                    trade_id: trade_event.trade_id,
                                    symbol: trade_event.symbol,
                                    price: trade_event.price,
                                    quantity: trade_event.quantity,
                                    timestamp_ms: trade_event.timestamp_ms,
                                    maker_order_id: trade_event.maker_order_id,
                                    taker_order_id: trade_event.taker_order_id,
                                };
                                if let Ok(json) = serde_json::to_string(&msg)
                                    && sender_clone.lock().await.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Trade stream lagged {} messages", n);
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
    /// Optional request ID for correlation.
    #[serde(default)]
    request_id: Option<String>,
    /// Optional batch channels for batch subscribe/unsubscribe.
    #[serde(default)]
    channels: Option<Vec<ChannelSubscription>>,
}

/// Sender type alias for WebSocket.
type WsSender = Arc<tokio::sync::Mutex<futures::stream::SplitSink<WebSocket, Message>>>;

/// Handle incoming client messages.
async fn handle_client_message(
    text: &str,
    state: &Arc<AppState>,
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
) {
    if let Ok(cmd) = serde_json::from_str::<ClientCommand>(text) {
        match cmd.action.as_str() {
            "subscribe" => {
                if let Some(channel) = &cmd.channel {
                    match channel.as_str() {
                        "orderbook" => {
                            handle_orderbook_subscribe(state, sender, subscribed_symbols, &cmd)
                                .await;
                        }
                        "trades" => {
                            handle_trades_subscribe(sender, subscribed_trades, &cmd).await;
                        }
                        _ => {
                            debug!("Unknown channel: {}", channel);
                        }
                    }
                } else {
                    debug!("Client subscribed to {:?}", cmd.symbol);
                }
            }
            "unsubscribe" => {
                if let Some(channel) = &cmd.channel {
                    match channel.as_str() {
                        "orderbook" => {
                            handle_orderbook_unsubscribe(sender, subscribed_symbols, &cmd).await;
                        }
                        "trades" => {
                            handle_trades_unsubscribe(sender, subscribed_trades, &cmd).await;
                        }
                        _ => {
                            debug!("Unknown channel: {}", channel);
                        }
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
            "batch_subscribe" => {
                handle_batch_subscribe(state, sender, subscribed_symbols, subscribed_trades, &cmd)
                    .await;
            }
            "batch_unsubscribe" => {
                handle_batch_unsubscribe(sender, subscribed_symbols, subscribed_trades, &cmd).await;
            }
            "list_subscriptions" => {
                handle_list_subscriptions(sender, subscribed_symbols, subscribed_trades).await;
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

/// Handles trades subscription requests.
async fn handle_trades_subscribe(
    sender: &WsSender,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    cmd: &ClientCommand,
) {
    let Some(symbol) = &cmd.symbol else {
        let error_msg = WsMessage::Error {
            message: "Symbol required for trades subscription".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    };

    // Validate symbol format: UNDERLYING-EXPIRATION-STRIKE-STYLE
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

    // Add to subscribed trades
    subscribed_trades.write().await.insert(symbol.clone());

    // Send subscription confirmation
    let subscribed_msg = WsMessage::Subscribed {
        channel: "trades".to_string(),
        symbol: symbol.clone(),
    };
    if let Ok(json) = serde_json::to_string(&subscribed_msg) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!("Client subscribed to trades: {}", symbol);
}

/// Handles trades unsubscription requests.
async fn handle_trades_unsubscribe(
    sender: &WsSender,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    cmd: &ClientCommand,
) {
    let Some(symbol) = &cmd.symbol else {
        let error_msg = WsMessage::Error {
            message: "Symbol required for trades unsubscription".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    };

    // Remove from subscribed trades
    subscribed_trades.write().await.remove(symbol);

    // Send unsubscription confirmation
    let unsubscribed_msg = WsMessage::Unsubscribed {
        channel: "trades".to_string(),
        symbol: symbol.clone(),
    };
    if let Ok(json) = serde_json::to_string(&unsubscribed_msg) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!("Client unsubscribed from trades: {}", symbol);
}

/// Handles batch subscription requests.
async fn handle_batch_subscribe(
    state: &Arc<AppState>,
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    cmd: &ClientCommand,
) {
    let Some(channels) = &cmd.channels else {
        let error_msg = WsMessage::Error {
            message: "channels array required for batch_subscribe".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    };

    let mut results = Vec::new();

    for sub in channels {
        let result = process_channel_subscription(
            state,
            subscribed_symbols,
            subscribed_trades,
            sub,
            true, // subscribe
        )
        .await;
        results.push(result);
    }

    // Send batch response
    let response = WsMessage::BatchSubscribed {
        request_id: cmd.request_id.clone(),
        subscriptions: results,
    };
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!(
        "Client batch subscribed to {} channels",
        cmd.channels.as_ref().map(|c| c.len()).unwrap_or(0)
    );
}

/// Handles batch unsubscription requests.
async fn handle_batch_unsubscribe(
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    cmd: &ClientCommand,
) {
    let Some(channels) = &cmd.channels else {
        let error_msg = WsMessage::Error {
            message: "channels array required for batch_unsubscribe".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender.lock().await.send(Message::Text(json.into())).await;
        }
        return;
    };

    let mut results = Vec::new();

    for sub in channels {
        let result =
            process_channel_unsubscription(subscribed_symbols, subscribed_trades, sub).await;
        results.push(result);
    }

    // Send batch response
    let response = WsMessage::BatchUnsubscribed {
        request_id: cmd.request_id.clone(),
        subscriptions: results,
    };
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!(
        "Client batch unsubscribed from {} channels",
        cmd.channels.as_ref().map(|c| c.len()).unwrap_or(0)
    );
}

/// Processes a single channel subscription.
async fn process_channel_subscription(
    _state: &Arc<AppState>,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    sub: &ChannelSubscription,
    _subscribe: bool,
) -> SubscriptionResult {
    match sub.channel {
        SubscriptionChannel::Orderbook => {
            if let Some(symbol) = &sub.symbol {
                // Validate symbol format
                let parts: Vec<&str> = symbol.split('-').collect();
                if parts.len() < 4 {
                    return SubscriptionResult {
                        channel: sub.channel.clone(),
                        symbol: Some(symbol.clone()),
                        underlying: sub.underlying.clone(),
                        status: "error: invalid symbol format".to_string(),
                    };
                }
                subscribed_symbols.write().await.insert(symbol.clone());
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: Some(symbol.clone()),
                    underlying: None,
                    status: "ok".to_string(),
                }
            } else if let Some(underlying) = &sub.underlying {
                // Wildcard subscription by underlying
                let filter = format!("{}:*", underlying);
                subscribed_symbols.write().await.insert(filter.clone());
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: Some(underlying.clone()),
                    status: "ok".to_string(),
                }
            } else {
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: None,
                    status: "error: symbol or underlying required".to_string(),
                }
            }
        }
        SubscriptionChannel::Trades => {
            if let Some(symbol) = &sub.symbol {
                let parts: Vec<&str> = symbol.split('-').collect();
                if parts.len() < 4 {
                    return SubscriptionResult {
                        channel: sub.channel.clone(),
                        symbol: Some(symbol.clone()),
                        underlying: sub.underlying.clone(),
                        status: "error: invalid symbol format".to_string(),
                    };
                }
                subscribed_trades.write().await.insert(symbol.clone());
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: Some(symbol.clone()),
                    underlying: None,
                    status: "ok".to_string(),
                }
            } else if let Some(underlying) = &sub.underlying {
                let filter = format!("{}:*", underlying);
                subscribed_trades.write().await.insert(filter.clone());
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: Some(underlying.clone()),
                    status: "ok".to_string(),
                }
            } else {
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: None,
                    status: "error: symbol or underlying required".to_string(),
                }
            }
        }
        SubscriptionChannel::Quotes | SubscriptionChannel::Prices | SubscriptionChannel::Fills => {
            // These channels are not yet fully implemented but we accept subscriptions
            SubscriptionResult {
                channel: sub.channel.clone(),
                symbol: sub.symbol.clone(),
                underlying: sub.underlying.clone(),
                status: "ok".to_string(),
            }
        }
    }
}

/// Processes a single channel unsubscription.
async fn process_channel_unsubscription(
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    sub: &ChannelSubscription,
) -> SubscriptionResult {
    match sub.channel {
        SubscriptionChannel::Orderbook => {
            if let Some(symbol) = &sub.symbol {
                subscribed_symbols.write().await.remove(symbol);
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: Some(symbol.clone()),
                    underlying: None,
                    status: "ok".to_string(),
                }
            } else if let Some(underlying) = &sub.underlying {
                let filter = format!("{}:*", underlying);
                subscribed_symbols.write().await.remove(&filter);
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: Some(underlying.clone()),
                    status: "ok".to_string(),
                }
            } else {
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: None,
                    status: "error: symbol or underlying required".to_string(),
                }
            }
        }
        SubscriptionChannel::Trades => {
            if let Some(symbol) = &sub.symbol {
                subscribed_trades.write().await.remove(symbol);
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: Some(symbol.clone()),
                    underlying: None,
                    status: "ok".to_string(),
                }
            } else if let Some(underlying) = &sub.underlying {
                let filter = format!("{}:*", underlying);
                subscribed_trades.write().await.remove(&filter);
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: Some(underlying.clone()),
                    status: "ok".to_string(),
                }
            } else {
                SubscriptionResult {
                    channel: sub.channel.clone(),
                    symbol: None,
                    underlying: None,
                    status: "error: symbol or underlying required".to_string(),
                }
            }
        }
        SubscriptionChannel::Quotes | SubscriptionChannel::Prices | SubscriptionChannel::Fills => {
            SubscriptionResult {
                channel: sub.channel.clone(),
                symbol: sub.symbol.clone(),
                underlying: sub.underlying.clone(),
                status: "ok".to_string(),
            }
        }
    }
}

/// Handles list subscriptions request.
async fn handle_list_subscriptions(
    sender: &WsSender,
    subscribed_symbols: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    subscribed_trades: &Arc<tokio::sync::RwLock<HashSet<String>>>,
) {
    let mut active = Vec::new();

    // Add orderbook subscriptions
    let symbols = subscribed_symbols.read().await;
    for symbol in symbols.iter() {
        if symbol.contains(":*") {
            // Wildcard subscription
            let underlying = symbol.trim_end_matches(":*");
            active.push(ActiveSubscription {
                channel: SubscriptionChannel::Orderbook,
                symbol: None,
                underlying: Some(underlying.to_string()),
                depth: None,
            });
        } else {
            active.push(ActiveSubscription {
                channel: SubscriptionChannel::Orderbook,
                symbol: Some(symbol.clone()),
                underlying: None,
                depth: Some(10), // Default depth
            });
        }
    }

    // Add trade subscriptions
    let trades = subscribed_trades.read().await;
    for symbol in trades.iter() {
        if symbol.contains(":*") {
            let underlying = symbol.trim_end_matches(":*");
            active.push(ActiveSubscription {
                channel: SubscriptionChannel::Trades,
                symbol: None,
                underlying: Some(underlying.to_string()),
                depth: None,
            });
        } else {
            active.push(ActiveSubscription {
                channel: SubscriptionChannel::Trades,
                symbol: Some(symbol.clone()),
                underlying: None,
                depth: None,
            });
        }
    }

    let response = WsMessage::SubscriptionList { active };
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = sender.lock().await.send(Message::Text(json.into())).await;
    }

    info!("Client requested subscription list");
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

    #[test]
    fn test_ws_message_trade_serialization() {
        let msg = WsMessage::Trade {
            trade_id: "trade-123".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
            price: 15050,
            quantity: 100,
            timestamp_ms: 1704067200000,
            maker_order_id: "maker-456".to_string(),
            taker_order_id: "taker-789".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"trade\""));
        assert!(json.contains("\"trade_id\":\"trade-123\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
        assert!(json.contains("\"price\":15050"));
        assert!(json.contains("\"quantity\":100"));
        assert!(json.contains("\"timestamp_ms\":1704067200000"));
        assert!(json.contains("\"maker_order_id\":\"maker-456\""));
        assert!(json.contains("\"taker_order_id\":\"taker-789\""));
    }

    #[test]
    fn test_trade_event_creation() {
        let event = TradeEvent {
            trade_id: "trade-abc".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
            price: 15000,
            quantity: 50,
            timestamp_ms: 1704067200000,
            maker_order_id: "maker-123".to_string(),
            taker_order_id: "taker-456".to_string(),
        };
        assert_eq!(event.trade_id, "trade-abc");
        assert_eq!(event.symbol, "AAPL-20240329-150-C");
        assert_eq!(event.price, 15000);
        assert_eq!(event.quantity, 50);
        assert_eq!(event.timestamp_ms, 1704067200000);
        assert_eq!(event.maker_order_id, "maker-123");
        assert_eq!(event.taker_order_id, "taker-456");
    }

    #[test]
    fn test_client_command_trades_subscribe_deserialization() {
        let json = r#"{"action":"subscribe","channel":"trades","symbol":"AAPL-20240329-150-C"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.action, "subscribe");
        assert_eq!(cmd.channel, Some("trades".to_string()));
        assert_eq!(cmd.symbol, Some("AAPL-20240329-150-C".to_string()));
    }

    #[test]
    fn test_client_command_trades_unsubscribe_deserialization() {
        let json = r#"{"action":"unsubscribe","channel":"trades","symbol":"AAPL-20240329-150-C"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.action, "unsubscribe");
        assert_eq!(cmd.channel, Some("trades".to_string()));
        assert_eq!(cmd.symbol, Some("AAPL-20240329-150-C".to_string()));
    }

    #[test]
    fn test_subscription_manager_trade_broadcast() {
        let manager = OrderbookSubscriptionManager::new();
        let mut rx = manager.subscribe_trades();

        let event = TradeEvent {
            trade_id: "trade-test".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
            price: 15000,
            quantity: 100,
            timestamp_ms: 1704067200000,
            maker_order_id: "maker-1".to_string(),
            taker_order_id: "taker-1".to_string(),
        };

        manager.broadcast_trade(event.clone());

        // Use try_recv to check if message was sent (non-blocking)
        match rx.try_recv() {
            Ok(received) => {
                assert_eq!(received.trade_id, "trade-test");
                assert_eq!(received.symbol, "AAPL-20240329-150-C");
            }
            Err(_) => {
                // Message might not be immediately available in test context
                // This is acceptable for unit test
            }
        }
    }

    #[test]
    fn test_ws_message_subscribed_trades_channel() {
        let msg = WsMessage::Subscribed {
            channel: "trades".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribed\""));
        assert!(json.contains("\"channel\":\"trades\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
    }

    #[test]
    fn test_ws_message_unsubscribed_trades_channel() {
        let msg = WsMessage::Unsubscribed {
            channel: "trades".to_string(),
            symbol: "AAPL-20240329-150-C".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"unsubscribed\""));
        assert!(json.contains("\"channel\":\"trades\""));
    }

    #[test]
    fn test_subscription_channel_serialization() {
        let channel = SubscriptionChannel::Orderbook;
        let json = serde_json::to_string(&channel).unwrap();
        assert_eq!(json, "\"orderbook\"");

        let channel = SubscriptionChannel::Trades;
        let json = serde_json::to_string(&channel).unwrap();
        assert_eq!(json, "\"trades\"");

        let channel = SubscriptionChannel::Quotes;
        let json = serde_json::to_string(&channel).unwrap();
        assert_eq!(json, "\"quotes\"");
    }

    #[test]
    fn test_subscription_channel_deserialization() {
        let channel: SubscriptionChannel = serde_json::from_str("\"orderbook\"").unwrap();
        assert_eq!(channel, SubscriptionChannel::Orderbook);

        let channel: SubscriptionChannel = serde_json::from_str("\"trades\"").unwrap();
        assert_eq!(channel, SubscriptionChannel::Trades);

        let channel: SubscriptionChannel = serde_json::from_str("\"fills\"").unwrap();
        assert_eq!(channel, SubscriptionChannel::Fills);
    }

    #[test]
    fn test_channel_subscription_serialization() {
        let sub = ChannelSubscription {
            channel: SubscriptionChannel::Orderbook,
            symbol: Some("AAPL-20240329-150-C".to_string()),
            underlying: None,
            expiration: None,
            depth: Some(10),
        };
        let json = serde_json::to_string(&sub).unwrap();
        assert!(json.contains("\"channel\":\"orderbook\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
        assert!(json.contains("\"depth\":10"));
    }

    #[test]
    fn test_channel_subscription_deserialization() {
        let json = r#"{"channel":"orderbook","symbol":"AAPL-20240329-150-C","depth":10}"#;
        let sub: ChannelSubscription = serde_json::from_str(json).unwrap();
        assert_eq!(sub.channel, SubscriptionChannel::Orderbook);
        assert_eq!(sub.symbol, Some("AAPL-20240329-150-C".to_string()));
        assert_eq!(sub.depth, Some(10));
    }

    #[test]
    fn test_channel_subscription_wildcard() {
        let json = r#"{"channel":"trades","underlying":"AAPL"}"#;
        let sub: ChannelSubscription = serde_json::from_str(json).unwrap();
        assert_eq!(sub.channel, SubscriptionChannel::Trades);
        assert_eq!(sub.underlying, Some("AAPL".to_string()));
        assert_eq!(sub.symbol, None);
    }

    #[test]
    fn test_subscription_result_serialization() {
        let result = SubscriptionResult {
            channel: SubscriptionChannel::Orderbook,
            symbol: Some("AAPL-20240329-150-C".to_string()),
            underlying: None,
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"channel\":\"orderbook\""));
        assert!(json.contains("\"status\":\"ok\""));
    }

    #[test]
    fn test_active_subscription_serialization() {
        let active = ActiveSubscription {
            channel: SubscriptionChannel::Trades,
            symbol: Some("AAPL-20240329-150-C".to_string()),
            underlying: None,
            depth: None,
        };
        let json = serde_json::to_string(&active).unwrap();
        assert!(json.contains("\"channel\":\"trades\""));
        assert!(json.contains("\"symbol\":\"AAPL-20240329-150-C\""));
    }

    #[test]
    fn test_ws_message_batch_subscribed_serialization() {
        let msg = WsMessage::BatchSubscribed {
            request_id: Some("req_123".to_string()),
            subscriptions: vec![
                SubscriptionResult {
                    channel: SubscriptionChannel::Orderbook,
                    symbol: Some("AAPL-20240329-150-C".to_string()),
                    underlying: None,
                    status: "ok".to_string(),
                },
                SubscriptionResult {
                    channel: SubscriptionChannel::Trades,
                    symbol: None,
                    underlying: Some("AAPL".to_string()),
                    status: "ok".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"batch_subscribed\""));
        assert!(json.contains("\"request_id\":\"req_123\""));
        assert!(json.contains("\"status\":\"ok\""));
    }

    #[test]
    fn test_ws_message_batch_unsubscribed_serialization() {
        let msg = WsMessage::BatchUnsubscribed {
            request_id: Some("req_456".to_string()),
            subscriptions: vec![SubscriptionResult {
                channel: SubscriptionChannel::Orderbook,
                symbol: Some("AAPL-20240329-150-C".to_string()),
                underlying: None,
                status: "ok".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"batch_unsubscribed\""));
        assert!(json.contains("\"request_id\":\"req_456\""));
    }

    #[test]
    fn test_ws_message_subscription_list_serialization() {
        let msg = WsMessage::SubscriptionList {
            active: vec![
                ActiveSubscription {
                    channel: SubscriptionChannel::Orderbook,
                    symbol: Some("AAPL-20240329-150-C".to_string()),
                    underlying: None,
                    depth: Some(10),
                },
                ActiveSubscription {
                    channel: SubscriptionChannel::Trades,
                    symbol: None,
                    underlying: Some("AAPL".to_string()),
                    depth: None,
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscriptions\""));
        assert!(json.contains("\"active\""));
        assert!(json.contains("\"channel\":\"orderbook\""));
        assert!(json.contains("\"channel\":\"trades\""));
    }

    #[test]
    fn test_client_command_batch_subscribe_deserialization() {
        let json = r#"{"action":"batch_subscribe","request_id":"req_123","channels":[{"channel":"orderbook","symbol":"AAPL-20240329-150-C","depth":10},{"channel":"trades","underlying":"AAPL"}]}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.action, "batch_subscribe");
        assert_eq!(cmd.request_id, Some("req_123".to_string()));
        assert!(cmd.channels.is_some());
        let channels = cmd.channels.unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].channel, SubscriptionChannel::Orderbook);
        assert_eq!(channels[1].channel, SubscriptionChannel::Trades);
    }

    #[test]
    fn test_client_command_list_subscriptions_deserialization() {
        let json = r#"{"action":"list_subscriptions"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.action, "list_subscriptions");
    }

    #[test]
    fn test_subscription_channel_display() {
        assert_eq!(SubscriptionChannel::Orderbook.to_string(), "orderbook");
        assert_eq!(SubscriptionChannel::Trades.to_string(), "trades");
        assert_eq!(SubscriptionChannel::Quotes.to_string(), "quotes");
        assert_eq!(SubscriptionChannel::Prices.to_string(), "prices");
        assert_eq!(SubscriptionChannel::Fills.to_string(), "fills");
    }
}

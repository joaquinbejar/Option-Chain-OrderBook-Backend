//! WebSocket handler for real-time updates.

use crate::market_maker::MarketMakerEvent;
use crate::state::AppState;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, error, info};

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
        bid_price: u64,
        /// Ask price in cents.
        ask_price: u64,
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
        price: u64,
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
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to market maker events
    let mut event_rx = state.market_maker.subscribe();

    // Send connection confirmation
    let connected_msg = WsMessage::Connected {
        message: "Connected to Option Chain OrderBook".to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    info!("WebSocket client connected");

    // Spawn task to handle incoming messages (for ping/pong and commands)
    let state_clone = Arc::clone(&state);
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);
                    // Handle client commands if needed
                    handle_client_message(&text, &state_clone).await;
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
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle market maker events
                event = event_rx.recv() => {
                    match event {
                        Ok(event) => {
                            if let Some(msg) = event_to_ws_message(event)
                                && let Ok(json) = serde_json::to_string(&msg)
                                    && sender.send(Message::Text(json.into())).await.is_err() {
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
                // Send periodic heartbeat
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                    let heartbeat = WsMessage::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    };
                    if let Ok(json) = serde_json::to_string(&heartbeat)
                        && sender.send(Message::Text(json.into())).await.is_err() {
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

/// Handle incoming client messages.
async fn handle_client_message(text: &str, state: &Arc<AppState>) {
    // Parse and handle client commands
    #[derive(serde::Deserialize)]
    struct ClientCommand {
        action: String,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        value: Option<f64>,
    }

    if let Ok(cmd) = serde_json::from_str::<ClientCommand>(text) {
        match cmd.action.as_str() {
            "subscribe" => {
                // Client wants to subscribe to specific symbol updates
                debug!("Client subscribed to {:?}", cmd.symbol);
            }
            "unsubscribe" => {
                debug!("Client unsubscribed from {:?}", cmd.symbol);
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

//! WebSocket client for real-time updates.

use crate::error::Error;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// WebSocket message types received from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Commands that can be sent to the server.
#[derive(Debug, Clone, Serialize)]
pub struct ClientCommand {
    /// Action to perform.
    pub action: String,
    /// Optional channel (e.g., "orderbook").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Optional symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Optional depth for orderbook subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Optional value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

impl ClientCommand {
    /// Creates a subscribe command.
    #[must_use]
    pub fn subscribe(symbol: &str) -> Self {
        Self {
            action: "subscribe".to_string(),
            channel: None,
            symbol: Some(symbol.to_string()),
            depth: None,
            value: None,
        }
    }

    /// Creates an orderbook subscribe command.
    #[must_use]
    pub fn subscribe_orderbook(symbol: &str, depth: Option<usize>) -> Self {
        Self {
            action: "subscribe".to_string(),
            channel: Some("orderbook".to_string()),
            symbol: Some(symbol.to_string()),
            depth,
            value: None,
        }
    }

    /// Creates an unsubscribe command.
    #[must_use]
    pub fn unsubscribe(symbol: &str) -> Self {
        Self {
            action: "unsubscribe".to_string(),
            channel: None,
            symbol: Some(symbol.to_string()),
            depth: None,
            value: None,
        }
    }

    /// Creates an orderbook unsubscribe command.
    #[must_use]
    pub fn unsubscribe_orderbook(symbol: &str) -> Self {
        Self {
            action: "unsubscribe".to_string(),
            channel: Some("orderbook".to_string()),
            symbol: Some(symbol.to_string()),
            depth: None,
            value: None,
        }
    }

    /// Creates a set_spread command.
    #[must_use]
    pub fn set_spread(value: f64) -> Self {
        Self {
            action: "set_spread".to_string(),
            channel: None,
            symbol: None,
            depth: None,
            value: Some(value),
        }
    }

    /// Creates a set_size command.
    #[must_use]
    pub fn set_size(value: f64) -> Self {
        Self {
            action: "set_size".to_string(),
            channel: None,
            symbol: None,
            depth: None,
            value: Some(value),
        }
    }

    /// Creates a set_skew command.
    #[must_use]
    pub fn set_skew(value: f64) -> Self {
        Self {
            action: "set_skew".to_string(),
            channel: None,
            symbol: None,
            depth: None,
            value: Some(value),
        }
    }

    /// Creates a kill command.
    #[must_use]
    pub fn kill() -> Self {
        Self {
            action: "kill".to_string(),
            channel: None,
            symbol: None,
            depth: None,
            value: None,
        }
    }

    /// Creates an enable command.
    #[must_use]
    pub fn enable() -> Self {
        Self {
            action: "enable".to_string(),
            channel: None,
            symbol: None,
            depth: None,
            value: None,
        }
    }
}

/// WebSocket client for receiving real-time updates.
pub struct WsClient {
    rx: mpsc::Receiver<WsMessage>,
    tx: mpsc::Sender<ClientCommand>,
}

impl WsClient {
    /// Connects to the WebSocket server.
    ///
    /// # Arguments
    /// * `url` - WebSocket URL (e.g., "ws://localhost:8080/ws")
    ///
    /// # Errors
    /// Returns error if connection fails.
    pub async fn connect(url: &str) -> Result<Self, Error> {
        let (ws_stream, _) = connect_async(url).await.map_err(Box::new)?;
        let (mut write, mut read) = ws_stream.split();

        // Channel for receiving messages
        let (msg_tx, msg_rx) = mpsc::channel::<WsMessage>(100);

        // Channel for sending commands
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientCommand>(100);

        // Spawn task to read messages
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text)
                            && msg_tx.send(ws_msg).await.is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
            }
        });

        // Spawn task to send commands
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                if let Ok(json) = serde_json::to_string(&cmd)
                    && write.send(Message::Text(json.into())).await.is_err()
                {
                    break;
                }
            }
        });

        Ok(Self {
            rx: msg_rx,
            tx: cmd_tx,
        })
    }

    /// Receives the next message from the server.
    ///
    /// Returns `None` if the connection is closed.
    pub async fn recv(&mut self) -> Option<WsMessage> {
        self.rx.recv().await
    }

    /// Sends a command to the server.
    ///
    /// # Errors
    /// Returns error if the send fails.
    pub async fn send(&self, cmd: ClientCommand) -> Result<(), Error> {
        self.tx.send(cmd).await.map_err(|_| Error::ConnectionClosed)
    }

    /// Subscribes to updates for a symbol.
    ///
    /// # Errors
    /// Returns error if the send fails.
    pub async fn subscribe(&self, symbol: &str) -> Result<(), Error> {
        self.send(ClientCommand::subscribe(symbol)).await
    }

    /// Unsubscribes from updates for a symbol.
    ///
    /// # Errors
    /// Returns error if the send fails.
    pub async fn unsubscribe(&self, symbol: &str) -> Result<(), Error> {
        self.send(ClientCommand::unsubscribe(symbol)).await
    }

    /// Subscribes to orderbook updates for a symbol.
    ///
    /// # Arguments
    /// * `symbol` - Symbol in format "UNDERLYING-EXPIRATION-STRIKE-STYLE"
    /// * `depth` - Optional depth (default: 10)
    ///
    /// # Errors
    /// Returns error if the send fails.
    pub async fn subscribe_orderbook(
        &self,
        symbol: &str,
        depth: Option<usize>,
    ) -> Result<(), Error> {
        self.send(ClientCommand::subscribe_orderbook(symbol, depth))
            .await
    }

    /// Unsubscribes from orderbook updates for a symbol.
    ///
    /// # Errors
    /// Returns error if the send fails.
    pub async fn unsubscribe_orderbook(&self, symbol: &str) -> Result<(), Error> {
        self.send(ClientCommand::unsubscribe_orderbook(symbol))
            .await
    }
}

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
}

/// Commands that can be sent to the server.
#[derive(Debug, Clone, Serialize)]
pub struct ClientCommand {
    /// Action to perform.
    pub action: String,
    /// Optional symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
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
            symbol: Some(symbol.to_string()),
            value: None,
        }
    }

    /// Creates an unsubscribe command.
    #[must_use]
    pub fn unsubscribe(symbol: &str) -> Self {
        Self {
            action: "unsubscribe".to_string(),
            symbol: Some(symbol.to_string()),
            value: None,
        }
    }

    /// Creates a set_spread command.
    #[must_use]
    pub fn set_spread(value: f64) -> Self {
        Self {
            action: "set_spread".to_string(),
            symbol: None,
            value: Some(value),
        }
    }

    /// Creates a set_size command.
    #[must_use]
    pub fn set_size(value: f64) -> Self {
        Self {
            action: "set_size".to_string(),
            symbol: None,
            value: Some(value),
        }
    }

    /// Creates a set_skew command.
    #[must_use]
    pub fn set_skew(value: f64) -> Self {
        Self {
            action: "set_skew".to_string(),
            symbol: None,
            value: Some(value),
        }
    }

    /// Creates a kill command.
    #[must_use]
    pub fn kill() -> Self {
        Self {
            action: "kill".to_string(),
            symbol: None,
            value: None,
        }
    }

    /// Creates an enable command.
    #[must_use]
    pub fn enable() -> Self {
        Self {
            action: "enable".to_string(),
            symbol: None,
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
}

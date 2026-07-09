//! Database schema types and queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Underlying price record from the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UnderlyingPrice {
    /// Unique identifier.
    pub id: Uuid,
    /// Underlying symbol (e.g., "SPY", "AAPL").
    pub symbol: String,
    /// Current price in cents (to avoid floating point issues).
    pub price_cents: i64,
    /// Bid price in cents.
    pub bid_cents: Option<i64>,
    /// Ask price in cents.
    pub ask_cents: Option<i64>,
    /// Volume.
    pub volume: Option<i64>,
    /// Timestamp when the price was recorded.
    pub timestamp: DateTime<Utc>,
    /// Source of the price data.
    pub source: Option<String>,
    /// Record creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Market maker configuration stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MarketMakerConfig {
    /// Unique identifier.
    pub id: Uuid,
    /// Underlying symbol this config applies to.
    pub symbol: String,
    /// Whether quoting is enabled for this symbol.
    pub quoting_enabled: bool,
    /// Spread multiplier (1.0 = normal spread).
    pub spread_multiplier: f64,
    /// Size scalar (percentage of max size to quote).
    pub size_scalar: f64,
    /// Directional skew (-1.0 to 1.0).
    pub directional_skew: f64,
    /// Maximum position size.
    pub max_position: i64,
    /// Maximum delta exposure.
    pub max_delta: f64,
    /// Last updated timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Execution record for audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Execution {
    /// Unique identifier.
    pub id: Uuid,
    /// Order ID that was filled.
    pub order_id: String,
    /// Underlying symbol.
    pub symbol: String,
    /// Option instrument (e.g., "SPY 450C 20240329").
    pub instrument: String,
    /// Side: "buy" or "sell".
    pub side: String,
    /// Executed quantity.
    pub quantity: i64,
    /// Execution price in cents.
    pub price_cents: i64,
    /// Theoretical value at execution time in cents.
    pub theo_value_cents: Option<i64>,
    /// Edge captured in cents.
    pub edge_cents: Option<i64>,
    /// Execution latency in microseconds.
    pub latency_us: Option<i64>,
    /// Execution timestamp.
    pub executed_at: DateTime<Utc>,
}

/// System control state.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SystemControl {
    /// Unique identifier (singleton row).
    pub id: i32,
    /// Master kill switch - if false, all quoting stops.
    pub master_enabled: bool,
    /// Global spread multiplier.
    pub global_spread_multiplier: f64,
    /// Global size scalar.
    pub global_size_scalar: f64,
    /// Global directional skew.
    pub global_directional_skew: f64,
    /// Last updated timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Default for SystemControl {
    fn default() -> Self {
        Self {
            id: 1,
            master_enabled: true,
            global_spread_multiplier: 1.0,
            global_size_scalar: 1.0,
            global_directional_skew: 0.0,
            updated_at: Utc::now(),
        }
    }
}

/// Request to insert an underlying price.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct InsertPriceRequest {
    /// Underlying symbol.
    pub symbol: String,
    /// Price (will be converted to cents internally).
    pub price: f64,
    /// Optional bid price.
    pub bid: Option<f64>,
    /// Optional ask price.
    pub ask: Option<f64>,
    /// Optional volume.
    pub volume: Option<i64>,
    /// Optional source identifier.
    pub source: Option<String>,
}

/// Request to update market maker parameters.
///
/// Field names are snake_case on the wire, matching every other DTO on the
/// public surface (issue #81 — previously these three fields were camelCase
/// while the matching responses were snake_case).
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateParametersRequest {
    /// Spread multiplier (optional).
    pub spread_multiplier: Option<f64>,
    /// Size scalar (optional).
    pub size_scalar: Option<f64>,
    /// Directional skew (optional).
    pub directional_skew: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #81: the control/price DTOs share one casing convention —
    /// snake_case — so a client can read a response and write the same field
    /// names back. The request round-trips through JSON with snake_case keys.
    #[test]
    fn test_update_parameters_request_snake_case_round_trip() {
        let request = UpdateParametersRequest {
            spread_multiplier: Some(1.5),
            size_scalar: Some(0.5),
            directional_skew: Some(-0.2),
        };

        let value = serde_json::to_value(&request).expect("serializes");
        assert!(value.get("spread_multiplier").is_some());
        assert!(value.get("size_scalar").is_some());
        assert!(value.get("directional_skew").is_some());
        assert!(
            value.get("spreadMultiplier").is_none(),
            "camelCase must be gone from the wire"
        );

        let back: UpdateParametersRequest = serde_json::from_value(value).expect("round-trips");
        assert_eq!(back.spread_multiplier, Some(1.5));
        assert_eq!(back.size_scalar, Some(0.5));
        assert_eq!(back.directional_skew, Some(-0.2));
    }

    /// InsertPriceRequest was already snake_case; lock it in.
    #[test]
    fn test_insert_price_request_snake_case() {
        let request = InsertPriceRequest {
            symbol: "BTC".to_string(),
            price: 100.5,
            bid: None,
            ask: None,
            volume: None,
            source: Some("test".to_string()),
        };
        let value = serde_json::to_value(&request).expect("serializes");
        assert!(value.get("symbol").is_some());
        assert!(value.get("price").is_some());
        assert!(value.get("source").is_some());
    }
}

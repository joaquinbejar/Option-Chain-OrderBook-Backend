//! Request and response models for the REST API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Order side for trading operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    /// Buy order.
    Buy,
    /// Sell order.
    Sell,
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buy => write!(f, "buy"),
            Self::Sell => write!(f, "sell"),
        }
    }
}

/// Market order execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarketOrderStatus {
    /// The order was fully filled.
    Filled,
    /// The order was partially filled.
    Partial,
    /// The order was rejected (no liquidity).
    Rejected,
}

impl std::fmt::Display for MarketOrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Filled => write!(f, "filled"),
            Self::Partial => write!(f, "partial"),
            Self::Rejected => write!(f, "rejected"),
        }
    }
}

/// Request to add a limit order.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct AddOrderRequest {
    /// Order side.
    pub side: OrderSide,
    /// Limit price in smallest units.
    pub price: u64,
    /// Order quantity in smallest units.
    pub quantity: u64,
}

/// Response after adding an order.
#[derive(Debug, Serialize, ToSchema)]
pub struct AddOrderResponse {
    /// The generated order ID.
    pub order_id: String,
    /// Success message.
    pub message: String,
}

/// Response for canceling an order.
#[derive(Debug, Serialize, ToSchema)]
pub struct CancelOrderResponse {
    /// Whether the order was successfully canceled.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
}

/// Quote information.
#[derive(Debug, Serialize, ToSchema)]
pub struct QuoteResponse {
    /// Best bid price.
    pub bid_price: Option<u64>,
    /// Best bid size.
    pub bid_size: u64,
    /// Best ask price.
    pub ask_price: Option<u64>,
    /// Best ask size.
    pub ask_size: u64,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// Order book snapshot.
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderBookSnapshotResponse {
    /// Symbol.
    pub symbol: String,
    /// Total bid depth.
    pub total_bid_depth: u64,
    /// Total ask depth.
    pub total_ask_depth: u64,
    /// Number of bid levels.
    pub bid_level_count: usize,
    /// Number of ask levels.
    pub ask_level_count: usize,
    /// Total order count.
    pub order_count: usize,
    /// Best quote.
    pub quote: QuoteResponse,
}

/// Underlying summary.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnderlyingSummary {
    /// Underlying symbol.
    pub symbol: String,
    /// Number of expirations.
    pub expiration_count: usize,
    /// Total strike count.
    pub total_strike_count: usize,
    /// Total order count.
    pub total_order_count: usize,
}

/// Expiration summary.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExpirationSummary {
    /// Expiration date string.
    pub expiration: String,
    /// Number of strikes.
    pub strike_count: usize,
    /// Total order count.
    pub total_order_count: usize,
}

/// Strike summary.
#[derive(Debug, Serialize, ToSchema)]
pub struct StrikeSummary {
    /// Strike price.
    pub strike: u64,
    /// Call order count.
    pub call_order_count: usize,
    /// Put order count.
    pub put_order_count: usize,
    /// Call quote.
    pub call_quote: QuoteResponse,
    /// Put quote.
    pub put_quote: QuoteResponse,
}

/// Global statistics.
#[derive(Debug, Serialize, ToSchema)]
pub struct GlobalStatsResponse {
    /// Number of underlyings.
    pub underlying_count: usize,
    /// Total expirations.
    pub total_expirations: usize,
    /// Total strikes.
    pub total_strikes: usize,
    /// Total orders.
    pub total_orders: usize,
}

/// List of underlyings.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnderlyingsListResponse {
    /// List of underlying symbols.
    pub underlyings: Vec<String>,
}

/// List of expirations.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExpirationsListResponse {
    /// List of expiration dates.
    pub expirations: Vec<String>,
}

/// List of strikes.
#[derive(Debug, Serialize, ToSchema)]
pub struct StrikesListResponse {
    /// List of strike prices.
    pub strikes: Vec<u64>,
}

/// Health check response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Service status.
    pub status: String,
    /// Service version.
    pub version: String,
}

/// Request to submit a market order.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct MarketOrderRequest {
    /// Order side.
    pub side: OrderSide,
    /// Order quantity in smallest units.
    pub quantity: u64,
}

/// Information about a single fill in a market order execution.
#[derive(Debug, Serialize, ToSchema)]
pub struct FillInfo {
    /// Execution price in smallest units.
    pub price: u64,
    /// Executed quantity in smallest units.
    pub quantity: u64,
}

/// Response after submitting a market order.
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketOrderResponse {
    /// The generated order ID.
    pub order_id: String,
    /// Order execution status.
    pub status: MarketOrderStatus,
    /// Total quantity that was filled.
    pub filled_quantity: u64,
    /// Remaining quantity that was not filled.
    pub remaining_quantity: u64,
    /// Average execution price (None if no fills).
    pub average_price: Option<f64>,
    /// List of individual fills.
    pub fills: Vec<FillInfo>,
}

// ============================================================================
// Enriched Snapshot Types
// ============================================================================

/// Depth parameter for snapshot requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapshotDepth {
    /// Only top of book (best bid/ask).
    #[default]
    Top,
    /// Specific number of levels.
    Levels(usize),
    /// Full depth (all levels).
    Full,
}

impl std::str::FromStr for SnapshotDepth {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "top" | "1" => Ok(Self::Top),
            "full" | "all" => Ok(Self::Full),
            other => other
                .parse::<usize>()
                .map(Self::Levels)
                .map_err(|_| format!("invalid depth: {}", other)),
        }
    }
}

impl SnapshotDepth {
    /// Converts depth to usize for API calls.
    #[must_use]
    pub fn to_usize(self) -> usize {
        match self {
            Self::Top => 1,
            Self::Levels(n) => n,
            Self::Full => usize::MAX,
        }
    }
}

/// Price level information in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PriceLevelInfo {
    /// Price in smallest units.
    pub price: u64,
    /// Total visible quantity at this level.
    pub quantity: u64,
    /// Number of orders at this level.
    pub order_count: usize,
}

/// Statistics for an enriched snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SnapshotStats {
    /// Mid price (average of best bid and ask).
    pub mid_price: Option<f64>,
    /// Spread in basis points.
    pub spread_bps: Option<f64>,
    /// Total depth on bid side.
    pub bid_depth_total: u64,
    /// Total depth on ask side.
    pub ask_depth_total: u64,
    /// Order book imbalance (-1.0 to 1.0).
    pub imbalance: f64,
    /// Volume-weighted average price for bids.
    pub vwap_bid: Option<f64>,
    /// Volume-weighted average price for asks.
    pub vwap_ask: Option<f64>,
}

/// Enriched order book snapshot response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EnrichedSnapshotResponse {
    /// Symbol identifier.
    pub symbol: String,
    /// Sequence number for incremental updates.
    pub sequence: u64,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Bid price levels (sorted by price descending).
    pub bids: Vec<PriceLevelInfo>,
    /// Ask price levels (sorted by price ascending).
    pub asks: Vec<PriceLevelInfo>,
    /// Pre-calculated statistics.
    pub stats: SnapshotStats,
}

/// Query parameters for snapshot endpoint.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SnapshotQuery {
    /// Depth parameter: "top" (default), "10", "20", or "full".
    #[serde(default)]
    pub depth: Option<String>,
}

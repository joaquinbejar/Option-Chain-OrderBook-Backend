//! Request and response types for the orderbook API.

use serde::{Deserialize, Serialize};

// ============================================================================
// Health & Stats
// ============================================================================

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status.
    pub status: String,
    /// Service version.
    pub version: String,
}

/// Global statistics response.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ============================================================================
// Underlyings
// ============================================================================

/// List of underlyings response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnderlyingsListResponse {
    /// List of underlying symbols.
    pub underlyings: Vec<String>,
}

/// Underlying summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ============================================================================
// Expirations
// ============================================================================

/// List of expirations response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpirationsListResponse {
    /// List of expiration dates.
    pub expirations: Vec<String>,
}

/// Expiration summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpirationSummary {
    /// Expiration date string.
    pub expiration: String,
    /// Number of strikes.
    pub strike_count: usize,
    /// Total order count.
    pub total_order_count: usize,
}

// ============================================================================
// Strikes
// ============================================================================

/// List of strikes response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrikesListResponse {
    /// List of strike prices.
    pub strikes: Vec<u64>,
}

/// Strike summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ============================================================================
// Quotes & OrderBook
// ============================================================================

/// Quote information.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Order book snapshot response.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ============================================================================
// Orders
// ============================================================================

/// Request to add a limit order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOrderRequest {
    /// Order side: "buy" or "sell".
    pub side: String,
    /// Limit price in smallest units.
    pub price: u64,
    /// Order quantity in smallest units.
    pub quantity: u64,
}

/// Response after adding an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOrderResponse {
    /// The generated order ID.
    pub order_id: String,
    /// Success message.
    pub message: String,
}

/// Response for canceling an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderResponse {
    /// Whether the order was successfully canceled.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
}

/// Request to submit a market order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOrderRequest {
    /// Order side: "buy" or "sell".
    pub side: String,
    /// Order quantity in smallest units.
    pub quantity: u64,
}

/// Information about a single fill in a market order execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillInfo {
    /// Execution price in smallest units.
    pub price: u64,
    /// Executed quantity in smallest units.
    pub quantity: u64,
}

/// Response after submitting a market order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOrderResponse {
    /// The generated order ID.
    pub order_id: String,
    /// Order status: "filled", "partial", or "rejected".
    pub status: String,
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
// Controls
// ============================================================================

/// Response for system control status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemControlResponse {
    /// Whether the master switch is enabled.
    pub master_enabled: bool,
    /// Global spread multiplier.
    pub spread_multiplier: f64,
    /// Global size scalar.
    pub size_scalar: f64,
    /// Global directional skew.
    pub directional_skew: f64,
}

/// Response for kill switch action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
    /// Current master enabled state.
    pub master_enabled: bool,
}

/// Request to update market maker parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateParametersRequest {
    /// Spread multiplier (optional).
    #[serde(rename = "spreadMultiplier", skip_serializing_if = "Option::is_none")]
    pub spread_multiplier: Option<f64>,
    /// Size scalar (optional).
    #[serde(rename = "sizeScalar", skip_serializing_if = "Option::is_none")]
    pub size_scalar: Option<f64>,
    /// Directional skew (optional).
    #[serde(rename = "directionalSkew", skip_serializing_if = "Option::is_none")]
    pub directional_skew: Option<f64>,
}

/// Response for parameter update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateParametersResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Updated spread multiplier.
    pub spread_multiplier: f64,
    /// Updated size scalar.
    pub size_scalar: f64,
    /// Updated directional skew.
    pub directional_skew: f64,
}

/// Response for instrument toggle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentToggleResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Symbol that was toggled.
    pub symbol: String,
    /// New enabled state.
    pub enabled: bool,
}

/// Instrument status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentStatus {
    /// Symbol.
    pub symbol: String,
    /// Whether quoting is enabled.
    pub quoting_enabled: bool,
    /// Current price (if available).
    pub current_price: Option<f64>,
}

/// Response for listing instruments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentsListResponse {
    /// List of instruments.
    pub instruments: Vec<InstrumentStatus>,
}

// ============================================================================
// Prices
// ============================================================================

/// Request to insert an underlying price.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertPriceRequest {
    /// Underlying symbol.
    pub symbol: String,
    /// Price (will be converted to cents internally).
    pub price: f64,
    /// Optional bid price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid: Option<f64>,
    /// Optional ask price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<f64>,
    /// Optional volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
    /// Optional source identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Response for price insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertPriceResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Symbol that was updated.
    pub symbol: String,
    /// Price in cents.
    pub price_cents: i64,
    /// Timestamp of the price.
    pub timestamp: String,
}

/// Response for latest price query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestPriceResponse {
    /// Symbol.
    pub symbol: String,
    /// Price in dollars.
    pub price: f64,
    /// Bid price in dollars (if available).
    pub bid: Option<f64>,
    /// Ask price in dollars (if available).
    pub ask: Option<f64>,
    /// Volume (if available).
    pub volume: Option<i64>,
    /// Timestamp.
    pub timestamp: String,
}

// ============================================================================
// Option Path Parameters
// ============================================================================

/// Parameters for identifying an option.
#[derive(Debug, Clone)]
pub struct OptionPath {
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date (YYYYMMDD format).
    pub expiration: String,
    /// Strike price.
    pub strike: u64,
    /// Option style: "call" or "put".
    pub style: String,
}

impl OptionPath {
    /// Creates a new option path.
    #[must_use]
    pub fn new(underlying: &str, expiration: &str, strike: u64, style: &str) -> Self {
        Self {
            underlying: underlying.to_string(),
            expiration: expiration.to_string(),
            strike,
            style: style.to_string(),
        }
    }

    /// Creates a call option path.
    #[must_use]
    pub fn call(underlying: &str, expiration: &str, strike: u64) -> Self {
        Self::new(underlying, expiration, strike, "call")
    }

    /// Creates a put option path.
    #[must_use]
    pub fn put(underlying: &str, expiration: &str, strike: u64) -> Self {
        Self::new(underlying, expiration, strike, "put")
    }
}

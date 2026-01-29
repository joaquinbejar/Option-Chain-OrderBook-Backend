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

/// Time in force for limit order requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum ApiTimeInForce {
    /// Good till canceled (default).
    #[default]
    Gtc,
    /// Immediate or cancel.
    Ioc,
    /// Fill or kill.
    Fok,
    /// Good till date.
    Gtd,
}

impl std::fmt::Display for ApiTimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gtc => write!(f, "GTC"),
            Self::Ioc => write!(f, "IOC"),
            Self::Fok => write!(f, "FOK"),
            Self::Gtd => write!(f, "GTD"),
        }
    }
}

/// Limit order execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LimitOrderStatus {
    /// Order was accepted and placed in the book.
    Accepted,
    /// Order was fully filled immediately.
    Filled,
    /// Order was partially filled (IOC behavior).
    Partial,
    /// Order was rejected (FOK not fillable, or other error).
    Rejected,
}

impl std::fmt::Display for LimitOrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted => write!(f, "accepted"),
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
    pub price: u128,
    /// Order quantity in smallest units.
    pub quantity: u64,
    /// Time in force (default: GTC).
    #[serde(default)]
    pub time_in_force: Option<ApiTimeInForce>,
    /// Expiration timestamp for GTD orders (ISO 8601 format).
    #[serde(default)]
    pub expire_at: Option<String>,
}

/// Response after adding an order.
#[derive(Debug, Serialize, ToSchema)]
pub struct AddOrderResponse {
    /// The generated order ID.
    pub order_id: String,
    /// Order execution status.
    pub status: LimitOrderStatus,
    /// Quantity that was filled immediately (for IOC/FOK).
    pub filled_quantity: u64,
    /// Remaining quantity in the book or unfilled.
    pub remaining_quantity: u64,
    /// Descriptive message.
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
    pub bid_price: Option<u128>,
    /// Best bid size.
    pub bid_size: u64,
    /// Best ask price.
    pub ask_price: Option<u128>,
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
    pub price: u128,
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
    pub price: u128,
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

// ============================================================================
// Last Trade Types
// ============================================================================

/// Response containing the last trade information for an option.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LastTradeResponse {
    /// The option symbol (e.g., "BTC-20240329-50000-C").
    pub symbol: String,
    /// Execution price in smallest units.
    pub price: u64,
    /// Executed quantity in smallest units.
    pub quantity: u64,
    /// The side of the taker (aggressor) in the trade.
    pub side: OrderSide,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Unique trade identifier.
    pub trade_id: String,
}

/// Internal storage for last trade information.
#[derive(Debug, Clone)]
pub struct LastTradeInfo {
    /// The option symbol.
    pub symbol: String,
    /// Execution price in smallest units.
    pub price: u64,
    /// Executed quantity in smallest units.
    pub quantity: u64,
    /// The side of the taker (aggressor) in the trade.
    pub side: OrderSide,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Unique trade identifier.
    pub trade_id: String,
}

impl From<LastTradeInfo> for LastTradeResponse {
    fn from(info: LastTradeInfo) -> Self {
        Self {
            symbol: info.symbol,
            price: info.price,
            quantity: info.quantity,
            side: info.side,
            timestamp_ms: info.timestamp_ms,
            trade_id: info.trade_id,
        }
    }
}

// ============================================================================
// Order Status and Query Types
// ============================================================================

/// Order status in the lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    /// Order is pending (not yet in the book).
    Pending,
    /// Order is active in the book.
    Active,
    /// Order is partially filled.
    Partial,
    /// Order is completely filled.
    Filled,
    /// Order was canceled.
    Canceled,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Partial => write!(f, "partial"),
            Self::Filled => write!(f, "filled"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

impl std::str::FromStr for OrderStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "partial" => Ok(Self::Partial),
            "filled" => Ok(Self::Filled),
            "canceled" | "cancelled" => Ok(Self::Canceled),
            _ => Err(format!("Invalid order status: {}", s)),
        }
    }
}

/// Time in force for orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderTimeInForce {
    /// Good till canceled.
    Gtc,
    /// Immediate or cancel.
    Ioc,
    /// Fill or kill.
    Fok,
    /// Good till date.
    Gtd,
}

impl std::fmt::Display for OrderTimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gtc => write!(f, "GTC"),
            Self::Ioc => write!(f, "IOC"),
            Self::Fok => write!(f, "FOK"),
            Self::Gtd => write!(f, "GTD"),
        }
    }
}

/// Information about a fill in an order.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderFillInfo {
    /// Execution price in smallest units.
    pub price: u128,
    /// Executed quantity.
    pub quantity: u64,
    /// Timestamp of the fill in milliseconds.
    pub timestamp_ms: u64,
}

/// Internal storage for order information.
#[derive(Debug, Clone)]
pub struct OrderInfo {
    /// Unique order identifier.
    pub order_id: String,
    /// Option symbol (e.g., "AAPL-20240329-150-C").
    pub symbol: String,
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date string.
    pub expiration: String,
    /// Strike price.
    pub strike: u64,
    /// Option style (call/put).
    pub style: String,
    /// Order side.
    pub side: OrderSide,
    /// Limit price in smallest units.
    pub price: u128,
    /// Original order quantity.
    pub original_quantity: u64,
    /// Remaining quantity.
    pub remaining_quantity: u64,
    /// Filled quantity.
    pub filled_quantity: u64,
    /// Current order status.
    pub status: OrderStatus,
    /// Time in force.
    pub time_in_force: OrderTimeInForce,
    /// Creation timestamp in milliseconds.
    pub created_at_ms: u64,
    /// Last update timestamp in milliseconds.
    pub updated_at_ms: u64,
    /// Fill history.
    pub fills: Vec<OrderFillInfo>,
}

/// Response for single order status query.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderStatusResponse {
    /// Unique order identifier.
    pub order_id: String,
    /// Option symbol (e.g., "AAPL-20240329-150-C").
    pub symbol: String,
    /// Order side.
    pub side: OrderSide,
    /// Limit price in smallest units.
    pub price: u128,
    /// Original order quantity.
    pub original_quantity: u64,
    /// Remaining quantity.
    pub remaining_quantity: u64,
    /// Filled quantity.
    pub filled_quantity: u64,
    /// Current order status.
    pub status: OrderStatus,
    /// Time in force.
    pub time_in_force: OrderTimeInForce,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last update timestamp (ISO 8601).
    pub updated_at: String,
    /// Fill history.
    pub fills: Vec<OrderFillInfo>,
}

impl From<OrderInfo> for OrderStatusResponse {
    fn from(info: OrderInfo) -> Self {
        use chrono::{TimeZone, Utc};
        Self {
            order_id: info.order_id,
            symbol: info.symbol,
            side: info.side,
            price: info.price,
            original_quantity: info.original_quantity,
            remaining_quantity: info.remaining_quantity,
            filled_quantity: info.filled_quantity,
            status: info.status,
            time_in_force: info.time_in_force,
            created_at: Utc
                .timestamp_millis_opt(info.created_at_ms as i64)
                .single()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            updated_at: Utc
                .timestamp_millis_opt(info.updated_at_ms as i64)
                .single()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            fills: info.fills,
        }
    }
}

/// Response for listing orders.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderListResponse {
    /// List of orders.
    pub orders: Vec<OrderStatusResponse>,
    /// Total number of matching orders.
    pub total: usize,
    /// Limit used for pagination.
    pub limit: usize,
    /// Offset used for pagination.
    pub offset: usize,
}

/// Query parameters for listing orders.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OrderListQuery {
    /// Filter by underlying symbol.
    #[serde(default)]
    pub underlying: Option<String>,
    /// Filter by order status.
    #[serde(default)]
    pub status: Option<String>,
    /// Filter by order side.
    #[serde(default)]
    pub side: Option<String>,
    /// Pagination limit (default: 100).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Pagination offset (default: 0).
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

// ============================================================================
// Position and Inventory Tracking Types
// ============================================================================

/// Internal storage for position information.
#[derive(Debug, Clone)]
pub struct PositionInfo {
    /// Option symbol (e.g., "AAPL-20240329-150-C").
    pub symbol: String,
    /// Underlying symbol.
    pub underlying: String,
    /// Position quantity (positive = long, negative = short).
    pub quantity: i64,
    /// Average entry price in smallest units.
    pub average_price: u128,
    /// Realized P&L in smallest units.
    pub realized_pnl: i64,
    /// Creation timestamp in milliseconds.
    pub created_at_ms: u64,
    /// Last update timestamp in milliseconds.
    pub updated_at_ms: u64,
}

impl PositionInfo {
    /// Creates a new position from a fill.
    #[must_use]
    pub fn new(
        symbol: String,
        underlying: String,
        quantity: i64,
        price: u128,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            symbol,
            underlying,
            quantity,
            average_price: price,
            realized_pnl: 0,
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
        }
    }

    /// Updates the position with a new fill.
    ///
    /// Returns the realized P&L from this fill (if closing a position).
    pub fn update(&mut self, fill_quantity: i64, fill_price: u128, timestamp_ms: u64) -> i64 {
        let mut realized = 0i64;

        // Check if this fill is closing or opening
        let same_direction =
            (self.quantity > 0 && fill_quantity > 0) || (self.quantity < 0 && fill_quantity < 0);

        if same_direction || self.quantity == 0 {
            // Opening or adding to position - update average price
            let old_value = self.average_price as i128 * self.quantity.abs() as i128;
            let new_value = fill_price as i128 * fill_quantity.abs() as i128;
            let total_quantity = self.quantity.abs() + fill_quantity.abs();

            if total_quantity > 0 {
                self.average_price = ((old_value + new_value) / total_quantity as i128) as u128;
            }
            self.quantity += fill_quantity;
        } else {
            // Closing position (opposite direction)
            let close_quantity = fill_quantity.abs().min(self.quantity.abs());

            // Calculate realized P&L
            let price_diff = fill_price as i128 - self.average_price as i128;
            if self.quantity > 0 {
                // Long position being closed by sell
                realized = (price_diff * close_quantity as i128) as i64;
            } else {
                // Short position being closed by buy
                realized = (-price_diff * close_quantity as i128) as i64;
            }

            self.realized_pnl += realized;
            self.quantity += fill_quantity;

            // If position flipped, reset average price to fill price
            if (self.quantity > 0 && fill_quantity > 0) || (self.quantity < 0 && fill_quantity < 0)
            {
                self.average_price = fill_price;
            }
        }

        self.updated_at_ms = timestamp_ms;
        realized
    }

    /// Calculates unrealized P&L given current market price.
    #[must_use]
    pub fn unrealized_pnl(&self, current_price: u128) -> i64 {
        let price_diff = current_price as i128 - self.average_price as i128;
        (price_diff * self.quantity as i128) as i64
    }

    /// Calculates notional value given current market price.
    #[must_use]
    pub fn notional_value(&self, current_price: u128) -> u128 {
        current_price * self.quantity.unsigned_abs() as u128
    }
}

/// Response for a single position.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionResponse {
    /// Option symbol (e.g., "AAPL-20240329-150-C").
    pub symbol: String,
    /// Underlying symbol.
    pub underlying: String,
    /// Position quantity (positive = long, negative = short).
    pub quantity: i64,
    /// Average entry price in smallest units.
    pub average_price: u128,
    /// Current market price in smallest units.
    pub current_price: u128,
    /// Unrealized P&L in smallest units.
    pub unrealized_pnl: i64,
    /// Realized P&L in smallest units.
    pub realized_pnl: i64,
    /// Delta exposure (quantity * delta).
    pub delta_exposure: f64,
    /// Notional value (current_price * abs(quantity)).
    pub notional_value: u128,
}

/// Summary of all positions.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionSummary {
    /// Total unrealized P&L across all positions.
    pub total_unrealized_pnl: i64,
    /// Total realized P&L across all positions.
    pub total_realized_pnl: i64,
    /// Net delta exposure across all positions.
    pub net_delta: f64,
    /// Number of open positions.
    pub position_count: usize,
}

/// Response for listing all positions.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionsListResponse {
    /// List of positions.
    pub positions: Vec<PositionResponse>,
    /// Aggregate summary.
    pub summary: PositionSummary,
}

/// Query parameters for listing positions.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PositionQuery {
    /// Filter by underlying symbol.
    #[serde(default)]
    pub underlying: Option<String>,
}

// ============================================================================
// OHLC (Candlestick) Data Types
// ============================================================================

/// OHLC bar interval for candlestick data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OhlcInterval {
    /// 1 minute bars.
    #[serde(rename = "1m")]
    OneMinute,
    /// 5 minute bars.
    #[serde(rename = "5m")]
    FiveMinutes,
    /// 15 minute bars.
    #[serde(rename = "15m")]
    FifteenMinutes,
    /// 1 hour bars.
    #[serde(rename = "1h")]
    OneHour,
    /// 4 hour bars.
    #[serde(rename = "4h")]
    FourHours,
    /// 1 day bars.
    #[serde(rename = "1d")]
    OneDay,
}

impl OhlcInterval {
    /// Returns the interval duration in seconds.
    #[must_use]
    pub fn seconds(&self) -> u64 {
        match self {
            Self::OneMinute => 60,
            Self::FiveMinutes => 300,
            Self::FifteenMinutes => 900,
            Self::OneHour => 3600,
            Self::FourHours => 14400,
            Self::OneDay => 86400,
        }
    }

    /// Floors a timestamp to the start of the interval.
    #[must_use]
    pub fn floor_timestamp(&self, timestamp_secs: u64) -> u64 {
        let interval = self.seconds();
        (timestamp_secs / interval) * interval
    }
}

impl std::fmt::Display for OhlcInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OneMinute => write!(f, "1m"),
            Self::FiveMinutes => write!(f, "5m"),
            Self::FifteenMinutes => write!(f, "15m"),
            Self::OneHour => write!(f, "1h"),
            Self::FourHours => write!(f, "4h"),
            Self::OneDay => write!(f, "1d"),
        }
    }
}

impl std::str::FromStr for OhlcInterval {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "1m" => Ok(Self::OneMinute),
            "5m" => Ok(Self::FiveMinutes),
            "15m" => Ok(Self::FifteenMinutes),
            "1h" => Ok(Self::OneHour),
            "4h" => Ok(Self::FourHours),
            "1d" => Ok(Self::OneDay),
            _ => Err(format!(
                "Invalid interval: {}. Use 1m, 5m, 15m, 1h, 4h, or 1d",
                s
            )),
        }
    }
}

/// A single OHLC bar (candlestick).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
pub struct OhlcBar {
    /// Bar start timestamp in seconds since epoch.
    pub timestamp: u64,
    /// Opening price in smallest units.
    pub open: u128,
    /// Highest price in smallest units.
    pub high: u128,
    /// Lowest price in smallest units.
    pub low: u128,
    /// Closing price in smallest units.
    pub close: u128,
    /// Total volume traded in this bar.
    pub volume: u64,
    /// Number of trades in this bar.
    pub trade_count: u64,
}

impl OhlcBar {
    /// Creates a new OHLC bar from a single trade.
    #[must_use]
    pub fn new(timestamp: u64, price: u128, quantity: u64) -> Self {
        Self {
            timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: quantity,
            trade_count: 1,
        }
    }

    /// Updates the bar with a new trade.
    pub fn update(&mut self, price: u128, quantity: u64) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        self.volume += quantity;
        self.trade_count += 1;
    }
}

/// Query parameters for OHLC endpoint.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OhlcQuery {
    /// Bar interval (1m, 5m, 15m, 1h, 4h, 1d).
    pub interval: String,
    /// Start timestamp in seconds (optional).
    #[serde(default)]
    pub from: Option<u64>,
    /// End timestamp in seconds (optional).
    #[serde(default)]
    pub to: Option<u64>,
    /// Maximum number of bars to return (default 500).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response for OHLC endpoint.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OhlcResponse {
    /// Symbol identifier.
    pub symbol: String,
    /// Bar interval.
    pub interval: String,
    /// List of OHLC bars.
    pub bars: Vec<OhlcBar>,
}

// ============================================================================
// Order Modification Types
// ============================================================================

/// Request to modify an existing order.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ModifyOrderRequest {
    /// New price for the order (optional).
    #[serde(default)]
    pub price: Option<u128>,
    /// New quantity for the order (optional).
    #[serde(default)]
    pub quantity: Option<u64>,
}

/// Status of an order modification request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModifyOrderStatus {
    /// Order was successfully modified.
    Modified,
    /// Order modification was rejected.
    Rejected,
}

impl std::fmt::Display for ModifyOrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Modified => write!(f, "modified"),
            Self::Rejected => write!(f, "rejected"),
        }
    }
}

/// Response after modifying an order.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModifyOrderResponse {
    /// The order ID that was modified.
    pub order_id: String,
    /// Status of the modification.
    pub status: ModifyOrderStatus,
    /// New price after modification (if changed).
    pub new_price: Option<u128>,
    /// New quantity after modification (if changed).
    pub new_quantity: Option<u64>,
    /// Whether the order lost time priority due to the modification.
    pub priority_changed: bool,
    /// Descriptive message.
    pub message: String,
}

// ============================================================================
// Bulk Order Operations Types
// ============================================================================

/// Individual order item in a bulk order request.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct BulkOrderItem {
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date string (e.g., "20240329").
    pub expiration: String,
    /// Strike price.
    pub strike: u64,
    /// Option style: "call" or "put".
    pub style: String,
    /// Order side: "buy" or "sell".
    pub side: String,
    /// Limit price.
    pub price: u128,
    /// Order quantity.
    pub quantity: u64,
}

/// Request for bulk order submission.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct BulkOrderRequest {
    /// List of orders to submit.
    pub orders: Vec<BulkOrderItem>,
    /// If true, all orders must succeed or none will be submitted.
    #[serde(default)]
    pub atomic: bool,
}

/// Status of an individual order in a bulk operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum BulkOrderStatus {
    /// Order was accepted.
    Accepted,
    /// Order was rejected.
    Rejected,
}

/// Result for an individual order in a bulk submission.
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkOrderResultItem {
    /// Index of the order in the request array.
    pub index: usize,
    /// Order ID if accepted.
    pub order_id: Option<String>,
    /// Status of the order.
    pub status: BulkOrderStatus,
    /// Error message if rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for bulk order submission.
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkOrderResponse {
    /// Number of orders successfully submitted.
    pub success_count: usize,
    /// Number of orders that failed.
    pub failure_count: usize,
    /// Detailed results for each order.
    pub results: Vec<BulkOrderResultItem>,
}

/// Request for bulk order cancellation.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct BulkCancelRequest {
    /// List of order IDs to cancel.
    pub order_ids: Vec<String>,
}

/// Result for an individual order cancellation.
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkCancelResultItem {
    /// Order ID that was attempted to cancel.
    pub order_id: String,
    /// Whether the cancellation was successful.
    pub canceled: bool,
    /// Error message if cancellation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for bulk order cancellation.
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkCancelResponse {
    /// Number of orders successfully canceled.
    pub success_count: usize,
    /// Number of orders that failed to cancel.
    pub failure_count: usize,
    /// Detailed results for each cancellation.
    pub results: Vec<BulkCancelResultItem>,
}

/// Query parameters for cancel-all endpoint.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CancelAllQuery {
    /// Filter by underlying symbol.
    #[serde(default)]
    pub underlying: Option<String>,
    /// Filter by expiration date.
    #[serde(default)]
    pub expiration: Option<String>,
    /// Filter by order side.
    #[serde(default)]
    pub side: Option<String>,
    /// Filter by option style.
    #[serde(default)]
    pub style: Option<String>,
}

/// Response for cancel-all endpoint.
#[derive(Debug, Serialize, ToSchema)]
pub struct CancelAllResponse {
    /// Number of orders successfully canceled.
    pub canceled_count: usize,
    /// Number of orders that failed to cancel.
    pub failed_count: usize,
}

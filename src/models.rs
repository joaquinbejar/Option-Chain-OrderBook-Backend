//! Request and response models for the REST API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Order side for trading operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(u8)]
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

/// Option style (call or put) as carried on the wire.
///
/// A typed enum so an invalid value is rejected during deserialization with a
/// `400` rather than being string-matched inside a handler. The wire form is
/// lowercase (`"call"` / `"put"`), matching the option-style path segment used
/// elsewhere in the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum OptionStyle {
    /// Call option.
    Call,
    /// Put option.
    Put,
}

impl std::fmt::Display for OptionStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Call => write!(f, "call"),
            Self::Put => write!(f, "put"),
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
#[repr(u8)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelOrderResponse {
    /// Whether the order was successfully canceled.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
}

/// Quote information.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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

/// Response for deleting an underlying.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeleteUnderlyingResponse {
    /// Whether the underlying was deleted.
    pub success: bool,
    /// Human-readable confirmation message.
    pub message: String,
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FillInfo {
    /// Execution price in smallest units.
    pub price: u128,
    /// Executed quantity in smallest units.
    pub quantity: u64,
}

/// Response after submitting a market order.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MarketOrderResponse {
    /// The generated order ID.
    pub order_id: String,
    /// Order execution status.
    pub status: MarketOrderStatus,
    /// Total quantity that was filled.
    pub filled_quantity: u64,
    /// Remaining quantity that was not filled.
    pub remaining_quantity: u64,
    /// Volume-weighted average execution price, in cents (`None` if no fills).
    ///
    /// Derived analytic float, intentionally exempt from the cents-as-integer
    /// money rule (see the crate-level "Money and analytic values" note): it is a
    /// computed average for display/analytics, not a settled monetary amount. The
    /// settled per-fill prices in [`FillInfo`] remain integer cents.
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
///
/// The price-derived fields here (`mid_price`, `spread_bps`, `vwap_bid`,
/// `vwap_ask`, `imbalance`) are derived analytic floats, intentionally exempt
/// from the cents-as-integer money rule (see the crate-level "Money and analytic
/// values" note): they are computed statistics for display/analytics, not
/// settled monetary amounts. Raw resting prices are carried as integer cents in
/// [`PriceLevelInfo`].
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
#[repr(u8)]
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
            created_at: i64::try_from(info.created_at_ms)
                .ok()
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            updated_at: i64::try_from(info.updated_at_ms)
                .ok()
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
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
    /// Returns the realized P&L (cents) from this fill (zero when the fill only
    /// opens or adds to the position).
    ///
    /// # Saturation
    /// The order book has already matched this fill, so an overflow here cannot be
    /// rejected. Every monetary computation (average entry price, realized P&L,
    /// cumulative realized P&L) uses checked arithmetic in a wide accumulator and,
    /// on overflow, SATURATES (`u128::MAX` / `i64::MAX` / `i64::MIN`) while
    /// emitting a single `tracing::warn!` that names the saturated field. Values
    /// are never silently wrapped and the function never panics. `quantity` is
    /// updated with `saturating_add` (an overflow there is structurally
    /// implausible for contract counts).
    pub fn update(&mut self, fill_quantity: i64, fill_price: u128, timestamp_ms: u64) -> i64 {
        let mut realized = 0i64;

        // Check if this fill is closing or opening
        let same_direction =
            (self.quantity > 0 && fill_quantity > 0) || (self.quantity < 0 && fill_quantity < 0);

        if same_direction || self.quantity == 0 {
            // Opening or adding to position - recompute the size-weighted average
            // entry price in u128. `total_quantity` is the post-fill open size and
            // is strictly positive whenever a divide is performed, so the division
            // cannot divide by zero.
            let old_qty = u128::from(self.quantity.unsigned_abs());
            let add_qty = u128::from(fill_quantity.unsigned_abs());
            let total_quantity = old_qty.saturating_add(add_qty);

            if total_quantity > 0 {
                let weighted = self
                    .average_price
                    .checked_mul(old_qty)
                    .zip(fill_price.checked_mul(add_qty))
                    .and_then(|(old_value, new_value)| old_value.checked_add(new_value))
                    .map(|sum| sum / total_quantity);

                self.average_price = match weighted {
                    Some(avg) => avg,
                    None => {
                        tracing::warn!(
                            symbol = %self.symbol,
                            "average_price computation overflowed u128; saturating to u128::MAX"
                        );
                        u128::MAX
                    }
                };
            }
            self.quantity = self.quantity.saturating_add(fill_quantity);
        } else {
            // Closing position (opposite direction)
            let close_quantity = i128::from(
                fill_quantity
                    .unsigned_abs()
                    .min(self.quantity.unsigned_abs()),
            );

            // Realized P&L in i128: (fill_price - average_price) * close_quantity,
            // negated for a short. The sign is captured up front so a full i128
            // overflow still saturates in the correct direction.
            let diff = i128::try_from(fill_price)
                .ok()
                .zip(i128::try_from(self.average_price).ok())
                .and_then(|(fp, ap)| fp.checked_sub(ap));
            let signed_diff = match diff {
                Some(d) if self.quantity > 0 => Some(d),
                Some(d) => d.checked_neg(),
                None => None,
            };
            let realized_i128 = signed_diff.and_then(|d| d.checked_mul(close_quantity));

            realized = match realized_i128.and_then(|v| i64::try_from(v).ok()) {
                Some(v) => v,
                None => {
                    // Saturate toward the sign of the (overflowing) realized P&L:
                    // profit -> i64::MAX, loss -> i64::MIN.
                    let positive = matches!(
                        (self.quantity > 0, fill_price.cmp(&self.average_price)),
                        (true, std::cmp::Ordering::Greater) | (false, std::cmp::Ordering::Less)
                    );
                    let clamped = if positive { i64::MAX } else { i64::MIN };
                    tracing::warn!(
                        symbol = %self.symbol,
                        "realized P&L computation overflowed i64; saturating to {clamped}"
                    );
                    clamped
                }
            };

            if self.realized_pnl.checked_add(realized).is_none() {
                tracing::warn!(
                    symbol = %self.symbol,
                    "cumulative realized_pnl overflowed i64; saturating"
                );
            }
            self.realized_pnl = self.realized_pnl.saturating_add(realized);
            self.quantity = self.quantity.saturating_add(fill_quantity);

            // If position flipped, reset average price to fill price
            if (self.quantity > 0 && fill_quantity > 0) || (self.quantity < 0 && fill_quantity < 0)
            {
                self.average_price = fill_price;
            }
        }

        self.updated_at_ms = timestamp_ms;
        realized
    }

    /// Calculates unrealized P&L (cents) given the current market price (cents).
    ///
    /// Computes `(current_price - average_price) * quantity` in `i128` with
    /// checked arithmetic, then narrows to `i64`.
    ///
    /// Returns `None` on arithmetic overflow — i.e. when the difference, the
    /// product, or the final narrowing to `i64` cannot be represented. `None`
    /// here means OVERFLOW, NOT an unpriced position: an unpriced position has no
    /// mark and so never calls this (the caller maps overflow to a typed internal
    /// error, distinct from the omitted-field unpriced representation).
    #[must_use]
    pub fn unrealized_pnl(&self, current_price: u128) -> Option<i64> {
        let current = i128::try_from(current_price).ok()?;
        let average = i128::try_from(self.average_price).ok()?;
        let price_diff = current.checked_sub(average)?;
        let pnl = price_diff.checked_mul(i128::from(self.quantity))?;
        i64::try_from(pnl).ok()
    }

    /// Calculates notional value (cents) given the current market price (cents).
    ///
    /// Computes `current_price * abs(quantity)` in `u128` with checked
    /// multiplication.
    ///
    /// Returns `None` on arithmetic overflow (the product exceeds `u128`). `None`
    /// here means OVERFLOW, NOT an unpriced position (see [`Self::unrealized_pnl`]).
    #[must_use]
    pub fn notional_value(&self, current_price: u128) -> Option<u128> {
        current_price.checked_mul(u128::from(self.quantity.unsigned_abs()))
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
    /// Average entry price in smallest units (cents).
    pub average_price: u128,
    /// Current market price in smallest units (cents).
    ///
    /// None/omitted when the symbol has no current quote (the position is
    /// unpriced; it is NOT marked at 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_price: Option<u128>,
    /// Unrealized P&L in smallest units (cents).
    ///
    /// None/omitted when the symbol has no current quote (the position is
    /// unpriced; no mark means no unrealized PnL can be computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl: Option<i64>,
    /// Realized P&L in smallest units (cents).
    pub realized_pnl: i64,
    /// Delta exposure (quantity * delta).
    pub delta_exposure: f64,
    /// Notional value (current_price * abs(quantity)) in smallest units (cents).
    ///
    /// None/omitted when the symbol has no current quote (the position is
    /// unpriced; it is NOT reported as 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notional_value: Option<u128>,
}

/// Summary of all positions.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionSummary {
    /// Total unrealized P&L (cents) across PRICED open positions only — those
    /// with a current quote. Unpriced open positions are excluded (they cannot
    /// be marked); `unpriced_count` reports how many were left out so the partial
    /// total is honest.
    pub total_unrealized_pnl: i64,
    /// Total realized P&L (cents) across all positions.
    pub total_realized_pnl: i64,
    /// Net delta exposure across PRICED open positions only.
    pub net_delta: f64,
    /// Number of open positions.
    pub position_count: usize,
    /// Number of open positions excluded from `total_unrealized_pnl` / `net_delta`
    /// because they have no current quote (unpriced).
    pub unpriced_count: usize,
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
#[repr(u8)]
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
    ///
    /// # Saturation
    /// Bars are aggregated from trades that have already executed, so an overflow
    /// of the running `volume` or `trade_count` cannot be rejected: both use
    /// `saturating_add` and emit a single `tracing::warn!` if they saturate at
    /// `u64::MAX` rather than wrapping or panicking.
    pub fn update(&mut self, price: u128, quantity: u64) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;

        if self.volume.checked_add(quantity).is_none() {
            tracing::warn!(
                timestamp = self.timestamp,
                "OHLC bar volume overflowed u64; saturating to u64::MAX"
            );
        }
        self.volume = self.volume.saturating_add(quantity);

        if self.trade_count.checked_add(1).is_none() {
            tracing::warn!(
                timestamp = self.timestamp,
                "OHLC bar trade_count overflowed u64; saturating to u64::MAX"
            );
        }
        self.trade_count = self.trade_count.saturating_add(1);
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
    /// Option style (call or put); invalid values are rejected with a 400.
    pub style: OptionStyle,
    /// Order side (buy or sell); invalid values are rejected with a 400.
    pub side: OrderSide,
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkOrderResponse {
    /// Number of orders accepted by the book (placed or filled).
    ///
    /// "Accepted" means the matching engine accepted the order — it is NOT
    /// necessarily still resting, since a marketable order can fill immediately
    /// on submit. For a non-atomic batch this is the count of accepted orders.
    /// After an atomic rollback it counts only the orders that could NOT be
    /// rolled back because they had already (partially) filled (a fill cannot be
    /// un-filled) — normally `0`; see `rollback_warnings`.
    pub success_count: usize,
    /// Number of orders not left in the live state after the request (rejected,
    /// not attempted, or cleanly rolled back).
    pub failure_count: usize,
    /// Detailed results for each order.
    pub results: Vec<BulkOrderResultItem>,
    /// True if an atomic rollback was performed because an order in the batch
    /// failed. When true, every previously-accepted order was cancelled in the
    /// real order book (best-effort — see `rollback_warnings`).
    pub rolled_back: bool,
    /// Best-effort rollback warnings.
    ///
    /// Populated during an atomic rollback when an accepted order could not be
    /// cleanly cancelled — most importantly when it had already (partially)
    /// filled, since a fill cannot be un-filled. Such orders remain live, are
    /// counted in `success_count`, and are reported with status `accepted`.
    ///
    /// `#[serde(default)]` so the field round-trips: it is omitted from the wire
    /// when empty (`skip_serializing_if`) and deserializes back to an empty Vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rollback_warnings: Vec<String>,
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

// ============================================================================
// Option Chain Matrix Types
// ============================================================================

/// Query parameters for option chain endpoint.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChainQuery {
    /// Minimum strike price filter.
    #[serde(default)]
    pub min_strike: Option<u64>,
    /// Maximum strike price filter.
    #[serde(default)]
    pub max_strike: Option<u64>,
}

/// Quote data for a single option (call or put).
#[derive(Debug, Clone, Serialize, ToSchema, Default)]
pub struct OptionQuoteData {
    /// Best bid price.
    pub bid: Option<u128>,
    /// Best ask price.
    pub ask: Option<u128>,
    /// Bid size (quantity at best bid).
    pub bid_size: u64,
    /// Ask size (quantity at best ask).
    pub ask_size: u64,
    /// Last trade price.
    pub last_trade: Option<u128>,
    /// Trading volume (placeholder, not tracked yet).
    pub volume: u64,
    /// Open interest (placeholder, not tracked yet).
    pub open_interest: u64,
    /// Delta (optional, if Greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    /// Gamma (optional, if Greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gamma: Option<f64>,
    /// Theta (optional, if Greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<f64>,
    /// Vega (optional, if Greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vega: Option<f64>,
    /// Implied volatility (optional, if calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iv: Option<f64>,
}

/// A single row in the option chain matrix (one strike with call and put).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChainStrikeRow {
    /// Strike price.
    pub strike: u64,
    /// Call option quote data.
    pub call: OptionQuoteData,
    /// Put option quote data.
    pub put: OptionQuoteData,
}

/// Response for option chain matrix endpoint.
#[derive(Debug, Serialize, ToSchema)]
pub struct OptionChainResponse {
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date string.
    pub expiration: String,
    /// Current spot price (if available).
    pub spot_price: Option<u128>,
    /// At-the-money strike (closest to spot price).
    pub atm_strike: Option<u64>,
    /// Chain data: list of strikes with call and put quotes.
    pub chain: Vec<ChainStrikeRow>,
}

// ============================================================================
// Greeks Types
// ============================================================================

/// Greeks data for an option.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GreeksData {
    /// Delta: rate of change of option price with respect to underlying price.
    pub delta: f64,
    /// Gamma: rate of change of delta with respect to underlying price.
    pub gamma: f64,
    /// Theta: rate of change of option price with respect to time (daily).
    pub theta: f64,
    /// Vega: rate of change of option price with respect to volatility.
    pub vega: f64,
    /// Rho: rate of change of option price with respect to interest rate.
    pub rho: f64,
    /// Vanna: sensitivity of delta to changes in volatility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanna: Option<f64>,
    /// Vomma: sensitivity of vega to changes in volatility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vomma: Option<f64>,
    /// Charm: rate of change of delta with respect to time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charm: Option<f64>,
    /// Color: rate of change of gamma with respect to time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<f64>,
}

impl Default for GreeksData {
    fn default() -> Self {
        Self {
            delta: 0.0,
            gamma: 0.0,
            theta: 0.0,
            vega: 0.0,
            rho: 0.0,
            vanna: None,
            vomma: None,
            charm: None,
            color: None,
        }
    }
}

/// Response for Greeks endpoint.
#[derive(Debug, Serialize, ToSchema)]
pub struct GreeksResponse {
    /// Option symbol.
    pub symbol: String,
    /// Greeks values.
    pub greeks: GreeksData,
    /// Implied volatility used for calculation.
    pub iv: f64,
    /// Theoretical option value.
    pub theoretical_value: f64,
    /// Timestamp of calculation in milliseconds.
    pub timestamp_ms: u64,
}

// ============================================================================
// Implied Volatility Surface Types
// ============================================================================

/// Implied volatility for a single strike (call and put).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct StrikeIV {
    /// Implied volatility for the call option.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_iv: Option<f64>,
    /// Implied volatility for the put option.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put_iv: Option<f64>,
}

/// A point in the ATM term structure.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ATMTermStructurePoint {
    /// Expiration date string.
    pub expiration: String,
    /// Days to expiration.
    pub days: u64,
    /// ATM implied volatility.
    pub iv: f64,
}

/// Response for volatility surface endpoint.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VolatilitySurfaceResponse {
    /// Underlying symbol.
    pub underlying: String,
    /// Current spot price (if available).
    pub spot_price: Option<u64>,
    /// Timestamp of calculation in milliseconds.
    pub timestamp_ms: u64,
    /// List of expiration dates.
    pub expirations: Vec<String>,
    /// List of strikes.
    pub strikes: Vec<u64>,
    /// IV surface: expiration -> strike -> StrikeIV.
    pub surface: std::collections::HashMap<String, std::collections::HashMap<u64, StrikeIV>>,
    /// ATM term structure.
    pub atm_term_structure: Vec<ATMTermStructurePoint>,
}

// ============================================================================
// Authentication Types (JWT + x509)
// ============================================================================

/// Permission levels carried by a JWT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Read-only access to market data.
    Read,
    /// Trading access (place/cancel orders).
    Trade,
    /// Administrative access (controls, snapshots, underlying deletion).
    Admin,
}

/// Request body for `POST /api/v1/auth/token`.
///
/// Gated by the operator bootstrap secret (`AUTH_BOOTSTRAP_SECRET`). Mints a JWT
/// carrying the requested `permissions`, valid for `ttl_secs` (or the server's
/// configured default when omitted).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TokenRequest {
    /// Operator bootstrap secret (`AUTH_BOOTSTRAP_SECRET`).
    pub secret: String,
    /// Permissions to embed in the token (Admin implies all).
    pub permissions: Vec<Permission>,
    /// Optional token lifetime in seconds (defaults to the server's TTL).
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

/// Response for `POST /api/v1/auth/token`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TokenResponse {
    /// The signed JWT (use as `Authorization: Bearer <token>`). Treat as a secret.
    pub token: String,
    /// Token expiration as an ISO-8601 / RFC3339 timestamp (UTC).
    pub expires_at: String,
}

// ============================================================================
// Orderbook Metrics Types
// ============================================================================

/// Spread metrics for an orderbook.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpreadMetrics {
    /// Current spread in price units.
    pub current: Option<u64>,
    /// Spread in basis points.
    pub spread_bps: Option<f64>,
}

/// Depth metrics for an orderbook.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DepthMetrics {
    /// Total bid depth (quantity).
    pub bid_depth_total: u64,
    /// Total ask depth (quantity).
    pub ask_depth_total: u64,
    /// Order book imbalance (-1 to 1, positive = more bids).
    pub imbalance: f64,
}

/// Price metrics for an orderbook.
///
/// Every field is a derived analytic float, intentionally exempt from the
/// cents-as-integer money rule (see the crate-level "Money and analytic values"
/// note): these are computed mid/micro/VWAP statistics for display/analytics,
/// not settled monetary amounts.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PriceMetrics {
    /// Mid price (average of best bid and ask).
    pub mid_price: Option<f64>,
    /// Micro price (volume-weighted mid price).
    pub micro_price: Option<f64>,
    /// Volume-weighted average price on bid side.
    pub vwap_bid: Option<f64>,
    /// Volume-weighted average price on ask side.
    pub vwap_ask: Option<f64>,
}

/// Market impact metrics for a single side.
///
/// `avg_price` and `slippage_bps` are derived analytic floats, intentionally
/// exempt from the cents-as-integer money rule (see the crate-level "Money and
/// analytic values" note): they are computed impact estimates for
/// display/analytics, not settled monetary amounts.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImpactMetrics {
    /// Average execution price.
    pub avg_price: Option<f64>,
    /// Slippage in basis points from mid price.
    pub slippage_bps: Option<f64>,
}

/// Market impact metrics for buy and sell sides.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MarketImpactMetrics {
    /// Impact for buying 100 units.
    pub buy_100: ImpactMetrics,
    /// Impact for selling 100 units.
    pub sell_100: ImpactMetrics,
}

/// Complete orderbook metrics response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OrderbookMetricsResponse {
    /// Option symbol.
    pub symbol: String,
    /// Timestamp of metrics calculation in milliseconds.
    pub timestamp_ms: u64,
    /// Spread metrics.
    pub spread: SpreadMetrics,
    /// Depth metrics.
    pub depth: DepthMetrics,
    /// Price metrics.
    pub prices: PriceMetrics,
    /// Market impact metrics.
    pub market_impact: MarketImpactMetrics,
}

// ============================================================================
// Execution Reports Types
// ============================================================================

/// Information about a single execution (fill).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionInfo {
    /// Unique execution identifier.
    pub execution_id: String,
    /// Order ID that was filled.
    pub order_id: String,
    /// Option symbol (e.g., "AAPL-20240329-150-C").
    pub symbol: String,
    /// Side of the execution.
    pub side: OrderSide,
    /// Execution price in cents.
    pub price: u64,
    /// Executed quantity.
    pub quantity: u64,
    /// Execution timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Counterparty order ID (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty_order_id: Option<String>,
    /// Whether this execution was as a maker (resting order).
    pub is_maker: bool,
    /// Fee charged for this execution.
    pub fee: u64,
    /// Edge captured (difference from fair value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<i64>,
}

/// Summary statistics for executions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExecutionSummary {
    /// Total number of executions.
    pub total_executions: u64,
    /// Total volume executed.
    pub total_volume: u64,
    /// Total edge captured.
    pub total_edge: i64,
    /// Ratio of maker executions (0.0 to 1.0).
    pub maker_ratio: f64,
}

/// Query parameters for listing executions.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ExecutionsQuery {
    /// Filter by start date (ISO 8601 format).
    #[serde(default)]
    pub from: Option<String>,
    /// Filter by end date (ISO 8601 format).
    #[serde(default)]
    pub to: Option<String>,
    /// Filter by underlying symbol.
    #[serde(default)]
    pub underlying: Option<String>,
    /// Filter by option symbol.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Filter by side (buy/sell).
    #[serde(default)]
    pub side: Option<String>,
    /// Maximum number of results.
    #[serde(default = "default_executions_limit")]
    pub limit: u64,
    /// Offset for pagination.
    #[serde(default)]
    pub offset: u64,
}

/// Default limit for executions query.
fn default_executions_limit() -> u64 {
    1000
}

/// Response for listing executions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExecutionsListResponse {
    /// List of executions.
    pub executions: Vec<ExecutionInfo>,
    /// Summary statistics.
    pub summary: ExecutionSummary,
}

// ============================================================================
// Rate Limiting Types
// ============================================================================

/// Rate limit information for response headers.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RateLimitInfo {
    /// Maximum requests allowed per window.
    pub limit: u32,
    /// Remaining requests in current window.
    pub remaining: u32,
    /// Unix timestamp when the rate limit resets.
    pub reset: u64,
}

/// Rate limit configuration.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RateLimitConfig {
    /// Default requests per minute for unauthenticated requests.
    #[serde(default = "default_requests_per_minute")]
    pub default_requests_per_minute: u32,
    /// Requests per minute for trading endpoints.
    #[serde(default = "default_trading_requests_per_minute")]
    pub trading_requests_per_minute: u32,
    /// WebSocket messages per second.
    #[serde(default = "default_websocket_messages_per_second")]
    pub websocket_messages_per_second: u32,
}

/// Default requests per minute.
fn default_requests_per_minute() -> u32 {
    1000
}

/// Default trading requests per minute.
fn default_trading_requests_per_minute() -> u32 {
    100
}

/// Default WebSocket messages per second.
fn default_websocket_messages_per_second() -> u32 {
    50
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            default_requests_per_minute: default_requests_per_minute(),
            trading_requests_per_minute: default_trading_requests_per_minute(),
            websocket_messages_per_second: default_websocket_messages_per_second(),
        }
    }
}

/// Response for rate limit exceeded.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RateLimitExceededResponse {
    /// Error message.
    pub error: String,
    /// Rate limit information.
    pub rate_limit: RateLimitInfo,
    /// Seconds until rate limit resets.
    pub retry_after: u64,
}

// ============================================================================
// Orderbook Persistence Types
// ============================================================================

/// Information about a stored orderbook snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderbookSnapshotInfo {
    /// Unique snapshot identifier.
    pub snapshot_id: String,
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date string.
    pub expiration: String,
    /// Strike price.
    pub strike: u64,
    /// Option style (call/put).
    pub style: String,
    /// Number of orders in the snapshot.
    pub order_count: u64,
    /// Number of bid levels.
    pub bid_levels: u64,
    /// Number of ask levels.
    pub ask_levels: u64,
    /// Snapshot data as JSON string.
    pub data: String,
    /// Creation timestamp in milliseconds.
    pub created_at: u64,
}

/// Response for creating a snapshot.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreateSnapshotResponse {
    /// `true` when every orderbook was saved; `false` when at least one
    /// orderbook was skipped (see `orderbooks_failed`), i.e. the snapshot is
    /// partial.
    pub success: bool,
    /// Unique snapshot identifier.
    pub snapshot_id: String,
    /// Number of orderbooks saved.
    pub orderbooks_saved: u64,
    /// Total number of orders saved.
    pub orders_saved: u64,
    /// Number of orderbooks skipped because their state failed to serialize.
    pub orderbooks_failed: u64,
    /// Timestamp of the snapshot.
    pub timestamp_ms: u64,
}

/// Response for listing snapshots.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SnapshotsListResponse {
    /// List of snapshot summaries.
    pub snapshots: Vec<SnapshotSummary>,
    /// Total number of snapshots.
    pub total: u64,
}

/// Summary of a snapshot for listing.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SnapshotSummary {
    /// Unique snapshot identifier.
    pub snapshot_id: String,
    /// Number of orderbooks in the snapshot.
    pub orderbook_count: u64,
    /// Total number of orders.
    pub total_orders: u64,
    /// Creation timestamp in milliseconds.
    pub created_at: u64,
}

/// Response for restoring a snapshot.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RestoreSnapshotResponse {
    /// `true` when every orderbook in the snapshot was restored; `false` when
    /// at least one orderbook was skipped (see `orderbooks_failed`), i.e. the
    /// restore is partial.
    pub success: bool,
    /// Snapshot ID that was restored.
    pub snapshot_id: String,
    /// Number of orderbooks restored.
    pub orderbooks_restored: u64,
    /// Total number of orders restored (as counted at snapshot time).
    pub orders_restored: u64,
    /// Number of orderbooks that could not be restored (unparseable data,
    /// invalid expiration/style, or an upstream restore failure).
    pub orderbooks_failed: u64,
    /// Timestamp of the restore operation.
    pub timestamp_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire casing of every small contract enum is fixed (semver). These
    /// assertions lock the JSON representation so a rename cannot slip through.
    #[test]
    fn test_enum_wire_casing_is_stable() {
        assert_eq!(serde_json::to_string(&OrderSide::Buy).unwrap(), "\"buy\"");
        assert_eq!(serde_json::to_string(&OrderSide::Sell).unwrap(), "\"sell\"");

        assert_eq!(
            serde_json::to_string(&OptionStyle::Call).unwrap(),
            "\"call\""
        );
        assert_eq!(serde_json::to_string(&OptionStyle::Put).unwrap(), "\"put\"");

        assert_eq!(
            serde_json::to_string(&OrderStatus::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&OrderStatus::Canceled).unwrap(),
            "\"canceled\""
        );

        assert_eq!(
            serde_json::to_string(&Permission::Read).unwrap(),
            "\"read\""
        );
        assert_eq!(
            serde_json::to_string(&Permission::Admin).unwrap(),
            "\"admin\""
        );

        // Time-in-force is UPPERCASE on the wire.
        assert_eq!(
            serde_json::to_string(&ApiTimeInForce::Gtc).unwrap(),
            "\"GTC\""
        );
        assert_eq!(
            serde_json::to_string(&ApiTimeInForce::Gtd).unwrap(),
            "\"GTD\""
        );

        // OHLC intervals use their explicit renames.
        assert_eq!(
            serde_json::to_string(&OhlcInterval::OneMinute).unwrap(),
            "\"1m\""
        );
        assert_eq!(
            serde_json::to_string(&OhlcInterval::OneDay).unwrap(),
            "\"1d\""
        );
    }

    /// Every contract enum round-trips through JSON unchanged.
    #[test]
    fn test_enum_json_round_trip() {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            let json = serde_json::to_string(&side).unwrap();
            assert_eq!(serde_json::from_str::<OrderSide>(&json).unwrap(), side);
        }
        for style in [OptionStyle::Call, OptionStyle::Put] {
            let json = serde_json::to_string(&style).unwrap();
            assert_eq!(serde_json::from_str::<OptionStyle>(&json).unwrap(), style);
        }
        for tif in [
            ApiTimeInForce::Gtc,
            ApiTimeInForce::Ioc,
            ApiTimeInForce::Fok,
            ApiTimeInForce::Gtd,
        ] {
            let json = serde_json::to_string(&tif).unwrap();
            assert_eq!(serde_json::from_str::<ApiTimeInForce>(&json).unwrap(), tif);
        }
    }

    /// The response DTOs now derive `Deserialize`, so they survive a
    /// serialize -> deserialize -> serialize round-trip byte-for-byte.
    #[test]
    fn test_response_dtos_round_trip() {
        let quote = QuoteResponse {
            bid_price: Some(1234),
            bid_size: 5,
            ask_price: None,
            ask_size: 0,
            timestamp_ms: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&quote).unwrap();
        let back: QuoteResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);

        let market = MarketOrderResponse {
            order_id: "ord-1".to_string(),
            status: MarketOrderStatus::Partial,
            filled_quantity: 3,
            remaining_quantity: 2,
            average_price: Some(101.5),
            fills: vec![FillInfo {
                price: 10150,
                quantity: 3,
            }],
        };
        let json = serde_json::to_string(&market).unwrap();
        let back: MarketOrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);

        let bulk = BulkOrderResponse {
            success_count: 1,
            failure_count: 0,
            results: vec![BulkOrderResultItem {
                index: 0,
                order_id: Some("ord-2".to_string()),
                status: BulkOrderStatus::Accepted,
                error: None,
            }],
            rolled_back: false,
            rollback_warnings: Vec::new(),
        };
        let json = serde_json::to_string(&bulk).unwrap();
        let back: BulkOrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }

    /// The instrument symbol `UNDERLYING-EXPIRATION-STRIKE-STYLE` splits back
    /// into its parts, and the style token round-trips through [`OptionStyle`].
    #[test]
    fn test_instrument_symbol_roundtrip() {
        for (style, token) in [(OptionStyle::Call, "C"), (OptionStyle::Put, "P")] {
            let symbol = format!("BTC-20251231-100000-{token}");
            let parts: Vec<&str> = symbol.split('-').collect();
            assert_eq!(parts.len(), 4);
            assert_eq!(parts[0], "BTC");
            assert_eq!(parts[1], "20251231");
            assert_eq!(parts[2].parse::<u64>().unwrap(), 100_000);
            let parsed_style = match parts[3] {
                "C" => OptionStyle::Call,
                "P" => OptionStyle::Put,
                other => panic!("unexpected style token: {other}"),
            };
            assert_eq!(parsed_style, style);
            // Re-formatting yields the original symbol.
            let reformatted = format!("{}-{}-{}-{}", parts[0], parts[1], parts[2], token);
            assert_eq!(reformatted, symbol);
        }
    }
}

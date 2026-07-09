//! Request and response types for the orderbook API.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Order side for trading operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Option style (call or put). Mirrors the server `OptionStyle`; serialized as
/// lowercase (`"call"` / `"put"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Response for deleting an underlying.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteUnderlyingResponse {
    /// Whether the underlying was deleted.
    pub success: bool,
    /// Human-readable confirmation message.
    pub message: String,
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
    /// Order side.
    pub side: OrderSide,
    /// Limit price in smallest units.
    pub price: u128,
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
    /// Order side.
    pub side: OrderSide,
    /// Order quantity in smallest units.
    pub quantity: u64,
}

/// Information about a single fill in a market order execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillInfo {
    /// Execution price in smallest units.
    pub price: u128,
    /// Executed quantity in smallest units.
    pub quantity: u64,
}

/// Response after submitting a market order.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
// Enriched Snapshot
// ============================================================================

/// Price level information in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelInfo {
    /// Price in smallest units.
    pub price: u128,
    /// Total visible quantity at this level.
    pub quantity: u64,
    /// Number of orders at this level.
    pub order_count: usize,
}

/// Statistics for an enriched snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread_multiplier: Option<f64>,
    /// Size scalar (optional; fraction of the base quote size, 0.0-1.0 — the
    /// same representation `GET /controls` reports).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_scalar: Option<f64>,
    /// Directional skew (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
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

// ============================================================================
// Authentication
// ============================================================================

/// Permission level carried by a JWT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Read-only access.
    Read,
    /// Trading access.
    Trade,
    /// Administrative access.
    Admin,
}

/// Request body for `POST /api/v1/auth/token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    /// Operator bootstrap secret (`AUTH_BOOTSTRAP_SECRET`).
    pub secret: String,
    /// Permissions to embed in the token.
    pub permissions: Vec<Permission>,
    /// Optional token lifetime in seconds (defaults to the server's TTL).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ttl_secs: Option<u64>,
}

/// Response for `POST /api/v1/auth/token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// The signed JWT (use as `Authorization: Bearer <token>`).
    pub token: String,
    /// Token expiration as an ISO-8601 / RFC3339 timestamp (UTC).
    pub expires_at: String,
}

// ============================================================================
// Executions
// ============================================================================

/// Execution information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionInfo {
    /// Unique execution identifier.
    pub execution_id: String,
    /// Order ID that was filled.
    pub order_id: String,
    /// Option symbol.
    pub symbol: String,
    /// Order side.
    pub side: OrderSide,
    /// Execution price in cents.
    pub price: u64,
    /// Executed quantity.
    pub quantity: u64,
    /// Execution timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Counterparty order ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty_order_id: Option<String>,
    /// Whether this was a maker execution.
    pub is_maker: bool,
    /// Fee charged for this execution.
    pub fee: u64,
    /// Edge captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<i64>,
}

/// Summary statistics for executions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    /// Total number of executions.
    pub total_executions: u64,
    /// Total volume executed.
    pub total_volume: u64,
    /// Total edge captured.
    pub total_edge: i64,
    /// Ratio of maker executions.
    pub maker_ratio: f64,
}

/// Query parameters for listing executions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionsQuery {
    /// Filter by start date (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Filter by end date (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Filter by underlying symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
    /// Filter by exact symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Filter by side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// Maximum number of results.
    #[serde(default = "default_executions_limit")]
    pub limit: u64,
    /// Offset for pagination.
    #[serde(default)]
    pub offset: u64,
}

fn default_executions_limit() -> u64 {
    1000
}

impl Default for ExecutionsQuery {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            underlying: None,
            symbol: None,
            side: None,
            limit: default_executions_limit(),
            offset: 0,
        }
    }
}

/// Response for listing executions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionsListResponse {
    /// List of executions.
    pub executions: Vec<ExecutionInfo>,
    /// Summary statistics.
    pub summary: ExecutionSummary,
}

// ============================================================================
// Positions
// ============================================================================

/// Position information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    /// Option symbol.
    pub symbol: String,
    /// Net quantity (positive = long, negative = short).
    pub quantity: i64,
    /// Average entry price in cents.
    pub average_price: u128,
    /// Realized P&L in cents.
    pub realized_pnl: i64,
    /// Last update timestamp in milliseconds.
    pub updated_at: u64,
}

/// Query parameters for listing positions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionQuery {
    /// Filter by underlying symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
}

/// Summary statistics for positions.
///
/// Mirrors the server `PositionSummary` DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSummary {
    /// Total unrealized P&L (cents) across PRICED open positions only — those
    /// with a current quote. Unpriced open positions are excluded;
    /// `unpriced_count` reports how many were left out.
    pub total_unrealized_pnl: i64,
    /// Total realized P&L (cents) across all positions.
    pub total_realized_pnl: i64,
    /// Net delta exposure across PRICED open positions only.
    pub net_delta: f64,
    /// Number of open positions.
    pub position_count: usize,
    /// Number of open positions excluded from `total_unrealized_pnl` /
    /// `net_delta` because they have no current quote (unpriced).
    pub unpriced_count: usize,
}

/// Response for listing positions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionsListResponse {
    /// List of positions.
    pub positions: Vec<PositionResponse>,
    /// Summary statistics.
    pub summary: PositionSummary,
}

/// Response for a single position.
///
/// Mirrors the server `PositionResponse` DTO. The mark-dependent fields
/// (`current_price`, `unrealized_pnl`, `notional_value`) are `None`/omitted when
/// the symbol has no current quote — an unpriced position is NOT fabricated at 0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionResponse {
    /// Option symbol (e.g., "AAPL-20240329-150-C").
    pub symbol: String,
    /// Underlying symbol.
    pub underlying: String,
    /// Position quantity (positive = long, negative = short).
    pub quantity: i64,
    /// Average entry price in cents.
    pub average_price: u128,
    /// Current market price in cents; `None`/omitted when the symbol is unpriced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_price: Option<u128>,
    /// Unrealized P&L in cents; `None`/omitted when the symbol is unpriced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl: Option<i64>,
    /// Realized P&L in cents.
    pub realized_pnl: i64,
    /// Delta exposure (quantity * delta).
    pub delta_exposure: f64,
    /// Notional value (current_price * abs(quantity)) in cents; `None`/omitted
    /// when the symbol is unpriced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notional_value: Option<u128>,
}

// ============================================================================
// Orderbook Snapshots (Persistence)
// ============================================================================

/// Information about a stored orderbook snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Defaults to 0 when talking to an older server that omits the field.
    #[serde(default)]
    pub orderbooks_failed: u64,
    /// Timestamp of the snapshot.
    pub timestamp_ms: u64,
}

/// Summary of a snapshot for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Response for listing snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotsListResponse {
    /// List of snapshot summaries.
    pub snapshots: Vec<SnapshotSummary>,
    /// Total number of snapshots.
    pub total: u64,
}

/// Response for restoring a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Number of orderbooks that could not be restored.
    /// Defaults to 0 when talking to an older server that omits the field.
    #[serde(default)]
    pub orderbooks_failed: u64,
    /// Timestamp of the restore operation.
    pub timestamp_ms: u64,
}

// ============================================================================
// Orders (Extended)
// ============================================================================

/// Order status in the lifecycle. Mirrors the server `OrderStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Time in force for orders. Mirrors the server `OrderTimeInForce`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Information about a single fill in an order's fill history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFillInfo {
    /// Execution price in smallest units.
    pub price: u128,
    /// Executed quantity.
    pub quantity: u64,
    /// Timestamp of the fill in milliseconds.
    pub timestamp_ms: u64,
}

/// Response for a single order status query.
///
/// Mirrors the server's flat `OrderStatusResponse`: all order fields are at the
/// top level (there is no `order` wrapper). Timestamps are ISO-8601 strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Query parameters for listing orders. Mirrors the server `OrderListQuery`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderListQuery {
    /// Filter by underlying symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
    /// Filter by status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OrderStatus>,
    /// Filter by side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// Maximum number of results.
    #[serde(default = "default_order_limit")]
    pub limit: usize,
    /// Offset for pagination.
    #[serde(default)]
    pub offset: usize,
}

fn default_order_limit() -> usize {
    100
}

impl Default for OrderListQuery {
    fn default() -> Self {
        Self {
            underlying: None,
            status: None,
            side: None,
            limit: default_order_limit(),
            offset: 0,
        }
    }
}

/// Response for listing orders. Mirrors the server `OrderListResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Request to modify an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyOrderRequest {
    /// New price (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<u128>,
    /// New quantity (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u64>,
}

/// Status of an order modification request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModifyOrderStatus {
    /// Order was successfully modified.
    Modified,
    /// Order modification was rejected.
    Rejected,
}

/// Response for modifying an order. Mirrors the server `ModifyOrderResponse`.
///
/// Modification is cancel-and-replace, so `order_id` is the id of the NEW resting
/// order on success and `priority_changed` is always `true` for a successful
/// modification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyOrderResponse {
    /// The resulting order ID (the new id on success).
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

/// Single order item in a bulk order request. Mirrors the server `BulkOrderItem`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderItem {
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date string.
    pub expiration: String,
    /// Strike price.
    pub strike: u64,
    /// Option style (call or put).
    pub style: OptionStyle,
    /// Order side.
    pub side: OrderSide,
    /// Limit price in cents.
    pub price: u128,
    /// Order quantity.
    pub quantity: u64,
}

/// Request for bulk order submission. Mirrors the server `BulkOrderRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderRequest {
    /// List of orders to submit.
    pub orders: Vec<BulkOrderItem>,
    /// If true, all orders must succeed or none are left resting (atomic).
    #[serde(default)]
    pub atomic: bool,
}

/// Status of an individual order in a bulk submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BulkOrderStatus {
    /// Order was accepted by the book.
    Accepted,
    /// Order was rejected.
    Rejected,
}

/// Result for a single order in bulk submission. Mirrors the server
/// `BulkOrderResultItem`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderResultItem {
    /// Index of the order in the request array.
    pub index: usize,
    /// Order ID (present when accepted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Status of the order.
    pub status: BulkOrderStatus,
    /// Error message (present when rejected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for bulk order submission. Mirrors the server `BulkOrderResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderResponse {
    /// Number of orders accepted by the book (placed or filled).
    pub success_count: usize,
    /// Number of orders not left in the live state after the request.
    pub failure_count: usize,
    /// Results for each order.
    pub results: Vec<BulkOrderResultItem>,
    /// True when an atomic rollback was performed because an order failed.
    pub rolled_back: bool,
    /// Best-effort rollback warnings (present only when non-empty).
    #[serde(default)]
    pub rollback_warnings: Vec<String>,
}

/// Request for bulk order cancellation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCancelRequest {
    /// List of order IDs to cancel.
    pub order_ids: Vec<String>,
}

/// Result for a single order in bulk cancellation. Mirrors the server
/// `BulkCancelResultItem`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCancelResultItem {
    /// Order ID that was attempted.
    pub order_id: String,
    /// Whether the cancellation succeeded.
    pub canceled: bool,
    /// Error message (present when the cancellation failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for bulk order cancellation. Mirrors the server `BulkCancelResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCancelResponse {
    /// Number of orders successfully canceled.
    pub success_count: usize,
    /// Number of orders that failed to cancel.
    pub failure_count: usize,
    /// Results for each order.
    pub results: Vec<BulkCancelResultItem>,
}

/// Query parameters for cancel-all. Mirrors the server `CancelAllQuery`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancelAllQuery {
    /// Filter by underlying symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying: Option<String>,
    /// Filter by expiration date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<String>,
    /// Filter by side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// Filter by option style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// Response for cancel-all. Mirrors the server `CancelAllResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelAllResponse {
    /// Number of orders successfully canceled.
    pub canceled_count: usize,
    /// Number of orders that failed to cancel.
    pub failed_count: usize,
}

// ============================================================================
// Greeks
// ============================================================================

/// Greeks data for an option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreeksData {
    /// Delta.
    pub delta: f64,
    /// Gamma.
    pub gamma: f64,
    /// Theta.
    pub theta: f64,
    /// Vega.
    pub vega: f64,
    /// Rho.
    pub rho: f64,
}

/// Response for option greeks. Mirrors the server `GreeksResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreeksResponse {
    /// Option symbol.
    pub symbol: String,
    /// Greeks data.
    pub greeks: GreeksData,
    /// Implied volatility used for the calculation.
    pub iv: f64,
    /// Theoretical option value.
    pub theoretical_value: f64,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

// ============================================================================
// Last Trade
// ============================================================================

/// Response for last trade. Mirrors the server's flat `LastTradeResponse`.
///
/// The server returns `404` (mapped to [`Error::NotFound`](crate::Error::NotFound))
/// when the option has no recorded trade, so a successful response always
/// describes a real trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTradeResponse {
    /// Option symbol.
    pub symbol: String,
    /// Trade price in cents.
    pub price: u64,
    /// Trade quantity.
    pub quantity: u64,
    /// Side of the taker (aggressor).
    pub side: OrderSide,
    /// Trade timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Unique trade identifier.
    pub trade_id: String,
}

// ============================================================================
// OHLC
// ============================================================================

/// OHLC bar data. Mirrors the server `OhlcBar`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhlcBar {
    /// Bar start time in seconds since epoch.
    pub timestamp: u64,
    /// Open price in cents.
    pub open: u128,
    /// High price in cents.
    pub high: u128,
    /// Low price in cents.
    pub low: u128,
    /// Close price in cents.
    pub close: u128,
    /// Volume.
    pub volume: u64,
    /// Number of trades.
    pub trade_count: u64,
}

/// Query parameters for OHLC.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OhlcQuery {
    /// Interval (1m, 5m, 15m, 1h, 1d).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    /// Start time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<u64>,
    /// End time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<u64>,
    /// Maximum number of bars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// Response for OHLC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhlcResponse {
    /// Option symbol.
    pub symbol: String,
    /// Interval.
    pub interval: String,
    /// OHLC bars.
    pub bars: Vec<OhlcBar>,
}

// ============================================================================
// Orderbook Metrics
// ============================================================================

/// Spread metrics. Mirrors the server `SpreadMetrics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadMetrics {
    /// Current spread in price units.
    pub current: Option<u64>,
    /// Spread in basis points.
    pub spread_bps: Option<f64>,
}

/// Depth metrics. Mirrors the server `DepthMetrics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthMetrics {
    /// Total bid depth (quantity).
    pub bid_depth_total: u64,
    /// Total ask depth (quantity).
    pub ask_depth_total: u64,
    /// Order book imbalance (-1 to 1, positive = more bids).
    pub imbalance: f64,
}

/// Price metrics. Mirrors the server `PriceMetrics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceMetrics {
    /// Mid price.
    pub mid_price: Option<f64>,
    /// Micro price.
    pub micro_price: Option<f64>,
    /// VWAP bid.
    pub vwap_bid: Option<f64>,
    /// VWAP ask.
    pub vwap_ask: Option<f64>,
}

/// Market impact metrics for buy and sell sides. Mirrors the server
/// `MarketImpactMetrics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketImpactMetrics {
    /// Impact for buying 100 units.
    pub buy_100: ImpactMetrics,
    /// Impact for selling 100 units.
    pub sell_100: ImpactMetrics,
}

/// Market impact metrics for a single side. Mirrors the server `ImpactMetrics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactMetrics {
    /// Average execution price.
    pub avg_price: Option<f64>,
    /// Slippage in basis points from the mid price.
    pub slippage_bps: Option<f64>,
}

/// Response for orderbook metrics. Mirrors the server `OrderbookMetricsResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookMetricsResponse {
    /// Option symbol.
    pub symbol: String,
    /// Timestamp in milliseconds.
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
// Volatility Surface
// ============================================================================

/// Strike IV data. Mirrors the server `StrikeIV`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrikeIV {
    /// Call implied volatility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_iv: Option<f64>,
    /// Put implied volatility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put_iv: Option<f64>,
}

/// A point in the ATM term structure. Mirrors the server `ATMTermStructurePoint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ATMTermStructurePoint {
    /// Expiration date string.
    pub expiration: String,
    /// Days to expiration.
    pub days: u64,
    /// ATM implied volatility.
    pub iv: f64,
}

/// Response for volatility surface. Mirrors the server `VolatilitySurfaceResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilitySurfaceResponse {
    /// Underlying symbol.
    pub underlying: String,
    /// Current spot price (if available).
    pub spot_price: Option<u64>,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// List of expirations.
    pub expirations: Vec<String>,
    /// List of strikes.
    pub strikes: Vec<u64>,
    /// Surface data: expiration -> strike -> StrikeIV.
    pub surface: std::collections::HashMap<String, std::collections::HashMap<u64, StrikeIV>>,
    /// ATM term structure.
    pub atm_term_structure: Vec<ATMTermStructurePoint>,
}

// ============================================================================
// Option Chain
// ============================================================================

/// Option quote data for a chain cell. Mirrors the server `OptionQuoteData`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionQuoteData {
    /// Best bid price.
    pub bid: Option<u128>,
    /// Best ask price.
    pub ask: Option<u128>,
    /// Bid size.
    pub bid_size: u64,
    /// Ask size.
    pub ask_size: u64,
    /// Last trade price.
    pub last_trade: Option<u128>,
    /// Volume.
    pub volume: u64,
    /// Open interest.
    pub open_interest: u64,
    /// Delta (present when greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    /// Gamma (present when greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gamma: Option<f64>,
    /// Theta (present when greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<f64>,
    /// Vega (present when greeks are calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vega: Option<f64>,
    /// Implied volatility (present when calculated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iv: Option<f64>,
}

/// Chain strike row. Mirrors the server `ChainStrikeRow`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStrikeRow {
    /// Strike price.
    pub strike: u64,
    /// Call data.
    pub call: OptionQuoteData,
    /// Put data.
    pub put: OptionQuoteData,
}

/// Response for option chain. Mirrors the server `OptionChainResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionChainResponse {
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date.
    pub expiration: String,
    /// Current spot price (if available).
    pub spot_price: Option<u128>,
    /// At-the-money strike (closest to spot price).
    pub atm_strike: Option<u64>,
    /// Chain rows.
    pub chain: Vec<ChainStrikeRow>,
}

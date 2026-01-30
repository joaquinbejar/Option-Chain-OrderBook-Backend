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
    /// Order side.
    pub side: OrderSide,
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
    /// Order side.
    pub side: OrderSide,
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
    pub price: u64,
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

// ============================================================================
// Authentication
// ============================================================================

/// Permission level for API keys.
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

/// Request to create an API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    /// Human-readable name for the key.
    pub name: String,
    /// Permissions granted to this key.
    pub permissions: Vec<Permission>,
    /// Rate limit in requests per minute.
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
}

fn default_rate_limit() -> u32 {
    1000
}

/// Response after creating an API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    /// Unique key identifier.
    pub key_id: String,
    /// The raw API key (only returned once).
    pub api_key: String,
    /// Human-readable name.
    pub name: String,
    /// Permissions granted.
    pub permissions: Vec<Permission>,
    /// Rate limit.
    pub rate_limit: u32,
    /// Creation timestamp in milliseconds.
    pub created_at: u64,
}

/// API key information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    /// Unique key identifier.
    pub key_id: String,
    /// Human-readable name.
    pub name: String,
    /// Permissions granted.
    pub permissions: Vec<Permission>,
    /// Rate limit.
    pub rate_limit: u32,
    /// Creation timestamp in milliseconds.
    pub created_at: u64,
    /// Last used timestamp in milliseconds.
    pub last_used_at: Option<u64>,
}

/// Response for listing API keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyListResponse {
    /// List of API keys.
    pub keys: Vec<ApiKeyInfo>,
}

/// Response for deleting an API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteApiKeyResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    pub average_price: u64,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSummary {
    /// Total number of positions.
    pub total_positions: u64,
    /// Total long positions.
    pub long_positions: u64,
    /// Total short positions.
    pub short_positions: u64,
    /// Total realized P&L.
    pub total_realized_pnl: i64,
}

/// Response for listing positions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionsListResponse {
    /// List of positions.
    pub positions: Vec<PositionInfo>,
    /// Summary statistics.
    pub summary: PositionSummary,
}

/// Response for a single position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionResponse {
    /// Position information.
    pub position: PositionInfo,
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
    /// Whether the operation was successful.
    pub success: bool,
    /// Unique snapshot identifier.
    pub snapshot_id: String,
    /// Number of orderbooks saved.
    pub orderbooks_saved: u64,
    /// Total number of orders saved.
    pub orders_saved: u64,
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
    /// Whether the operation was successful.
    pub success: bool,
    /// Snapshot ID that was restored.
    pub snapshot_id: String,
    /// Number of orderbooks restored.
    pub orderbooks_restored: u64,
    /// Total number of orders restored.
    pub orders_restored: u64,
    /// Timestamp of the restore operation.
    pub timestamp_ms: u64,
}

// ============================================================================
// Orders (Extended)
// ============================================================================

/// Order status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    /// Order is active.
    Active,
    /// Order was filled.
    Filled,
    /// Order was canceled.
    Canceled,
    /// Order expired.
    Expired,
}

/// Order information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    /// Order ID.
    pub order_id: String,
    /// Option symbol.
    pub symbol: String,
    /// Order side.
    pub side: OrderSide,
    /// Limit price in cents.
    pub price: u64,
    /// Original quantity.
    pub quantity: u64,
    /// Filled quantity.
    pub filled_quantity: u64,
    /// Order status.
    pub status: OrderStatus,
    /// Creation timestamp in milliseconds.
    pub created_at: u64,
    /// Last update timestamp in milliseconds.
    pub updated_at: u64,
}

/// Query parameters for listing orders.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderListQuery {
    /// Filter by symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Filter by side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// Filter by status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OrderStatus>,
    /// Maximum number of results.
    #[serde(default = "default_order_limit")]
    pub limit: u64,
    /// Offset for pagination.
    #[serde(default)]
    pub offset: u64,
}

fn default_order_limit() -> u64 {
    1000
}

/// Response for listing orders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderListResponse {
    /// List of orders.
    pub orders: Vec<OrderInfo>,
    /// Total count.
    pub total: u64,
}

/// Response for order status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatusResponse {
    /// Order information.
    pub order: OrderInfo,
}

/// Request to modify an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyOrderRequest {
    /// New price (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<u64>,
    /// New quantity (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u64>,
}

/// Response for modifying an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyOrderResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
    /// Updated order information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<OrderInfo>,
}

/// Request for bulk order submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderRequest {
    /// List of orders to submit.
    pub orders: Vec<BulkOrderItem>,
}

/// Single order in a bulk request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderItem {
    /// Option symbol.
    pub symbol: String,
    /// Order side.
    pub side: OrderSide,
    /// Limit price in cents.
    pub price: u64,
    /// Order quantity.
    pub quantity: u64,
}

/// Response for bulk order submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderResponse {
    /// Number of orders submitted.
    pub submitted: u64,
    /// Number of orders that failed.
    pub failed: u64,
    /// Results for each order.
    pub results: Vec<BulkOrderResultItem>,
}

/// Result for a single order in bulk submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOrderResultItem {
    /// Order ID (if successful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Whether the order was successful.
    pub success: bool,
    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request for bulk order cancellation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCancelRequest {
    /// List of order IDs to cancel.
    pub order_ids: Vec<String>,
}

/// Response for bulk order cancellation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCancelResponse {
    /// Number of orders canceled.
    pub canceled: u64,
    /// Number of orders that failed.
    pub failed: u64,
    /// Results for each order.
    pub results: Vec<BulkCancelResultItem>,
}

/// Result for a single order in bulk cancellation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkCancelResultItem {
    /// Order ID.
    pub order_id: String,
    /// Whether the cancellation was successful.
    pub success: bool,
    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Query parameters for cancel-all.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancelAllQuery {
    /// Filter by symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Filter by side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
}

/// Response for cancel-all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelAllResponse {
    /// Number of orders canceled.
    pub canceled: u64,
    /// Message describing the result.
    pub message: String,
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

/// Response for option greeks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreeksResponse {
    /// Option symbol.
    pub symbol: String,
    /// Greeks data.
    pub greeks: GreeksData,
    /// Underlying price used for calculation.
    pub underlying_price: f64,
    /// Implied volatility used.
    pub implied_volatility: f64,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

// ============================================================================
// Last Trade
// ============================================================================

/// Last trade information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTradeInfo {
    /// Option symbol.
    pub symbol: String,
    /// Trade price in cents.
    pub price: u64,
    /// Trade quantity.
    pub quantity: u64,
    /// Trade side.
    pub side: OrderSide,
    /// Trade timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// Response for last trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTradeResponse {
    /// Last trade information.
    pub trade: Option<LastTradeInfo>,
}

// ============================================================================
// OHLC
// ============================================================================

/// OHLC bar data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhlcBar {
    /// Bar open time in milliseconds.
    pub timestamp_ms: u64,
    /// Open price in cents.
    pub open: u64,
    /// High price in cents.
    pub high: u64,
    /// Low price in cents.
    pub low: u64,
    /// Close price in cents.
    pub close: u64,
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

/// Spread metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadMetrics {
    /// Absolute spread in cents.
    pub spread_absolute: Option<u64>,
    /// Spread in basis points.
    pub spread_bps: Option<f64>,
    /// Spread as percentage.
    pub spread_percent: Option<f64>,
}

/// Depth metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthMetrics {
    /// Total bid depth.
    pub bid_depth: u64,
    /// Total ask depth.
    pub ask_depth: u64,
    /// Order book imbalance.
    pub imbalance: f64,
}

/// Price metrics.
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

/// Market impact metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketImpactMetrics {
    /// Impact for buying 100 units.
    pub buy_100: Option<ImpactMetrics>,
    /// Impact for selling 100 units.
    pub sell_100: Option<ImpactMetrics>,
}

/// Impact metrics for a specific quantity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactMetrics {
    /// Average execution price.
    pub average_price: f64,
    /// Price impact in basis points.
    pub impact_bps: f64,
    /// Total cost.
    pub total_cost: u64,
}

/// Response for orderbook metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookMetricsResponse {
    /// Option symbol.
    pub symbol: String,
    /// Spread metrics.
    pub spread: SpreadMetrics,
    /// Depth metrics.
    pub depth: DepthMetrics,
    /// Price metrics.
    pub prices: PriceMetrics,
    /// Market impact metrics.
    pub impact: MarketImpactMetrics,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

// ============================================================================
// Volatility Surface
// ============================================================================

/// Strike IV data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrikeIV {
    /// Call implied volatility.
    pub call_iv: Option<f64>,
    /// Put implied volatility.
    pub put_iv: Option<f64>,
}

/// Response for volatility surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilitySurfaceResponse {
    /// Underlying symbol.
    pub underlying: String,
    /// Underlying price.
    pub underlying_price: Option<f64>,
    /// List of expirations.
    pub expirations: Vec<String>,
    /// List of strikes.
    pub strikes: Vec<u64>,
    /// Surface data: expiration -> strike -> StrikeIV.
    pub surface: std::collections::HashMap<String, std::collections::HashMap<u64, StrikeIV>>,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

// ============================================================================
// Option Chain
// ============================================================================

/// Option quote data for chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionQuoteData {
    /// Bid price.
    pub bid: Option<u64>,
    /// Ask price.
    pub ask: Option<u64>,
    /// Bid size.
    pub bid_size: u64,
    /// Ask size.
    pub ask_size: u64,
    /// Last trade price.
    pub last: Option<u64>,
    /// Volume.
    pub volume: u64,
    /// Open interest.
    pub open_interest: u64,
    /// Implied volatility.
    pub iv: Option<f64>,
    /// Delta.
    pub delta: Option<f64>,
}

/// Chain strike row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStrikeRow {
    /// Strike price.
    pub strike: u64,
    /// Call data.
    pub call: OptionQuoteData,
    /// Put data.
    pub put: OptionQuoteData,
}

/// Response for option chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionChainResponse {
    /// Underlying symbol.
    pub underlying: String,
    /// Expiration date.
    pub expiration: String,
    /// Underlying price.
    pub underlying_price: Option<f64>,
    /// Chain rows.
    pub chain: Vec<ChainStrikeRow>,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

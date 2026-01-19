//! API request handlers.

use crate::error::ApiError;
use crate::models::{
    AddOrderRequest, AddOrderResponse, CancelOrderResponse, EnrichedSnapshotResponse,
    ExpirationSummary, ExpirationsListResponse, FillInfo, GlobalStatsResponse, HealthResponse,
    MarketOrderRequest, MarketOrderResponse, MarketOrderStatus, OrderBookSnapshotResponse,
    OrderSide, PriceLevelInfo, QuoteResponse, SnapshotDepth, SnapshotQuery, SnapshotStats,
    StrikeSummary, StrikesListResponse, UnderlyingSummary, UnderlyingsListResponse,
};
use crate::state::AppState;
use axum::Json;
use axum::extract::Query;
use axum::extract::{Path, State};
use option_chain_orderbook::orderbook::Quote;
use optionstratlib::{ExpirationDate, OptionStyle};
use orderbook_rs::{OrderId, Side};
use std::sync::Arc;

/// Converts a Quote to QuoteResponse.
fn quote_to_response(quote: &Quote) -> QuoteResponse {
    QuoteResponse {
        bid_price: quote.bid_price(),
        bid_size: quote.bid_size(),
        ask_price: quote.ask_price(),
        ask_size: quote.ask_size(),
        timestamp_ms: quote.timestamp_ms(),
    }
}

/// Converts OrderSide to orderbook_rs::Side.
fn order_side_to_side(side: OrderSide) -> Side {
    match side {
        OrderSide::Buy => Side::Buy,
        OrderSide::Sell => Side::Sell,
    }
}

/// Parses option style string to OptionStyle enum.
fn parse_option_style(style: &str) -> Result<OptionStyle, ApiError> {
    match style.to_lowercase().as_str() {
        "call" | "c" => Ok(OptionStyle::Call),
        "put" | "p" => Ok(OptionStyle::Put),
        _ => Err(ApiError::InvalidRequest(format!(
            "Invalid option style: {}. Use 'call' or 'put'",
            style
        ))),
    }
}

/// Formats ExpirationDate to YYYYMMDD string for API responses.
fn format_expiration(exp: &ExpirationDate) -> String {
    match exp.get_date() {
        Ok(date) => date.format("%Y%m%d").to_string(),
        Err(_) => exp.to_string(),
    }
}

/// Finds an expiration in the underlying book by matching the formatted date string.
/// This is needed because ExpirationDate comparison uses get_days() which depends on current time.
fn find_expiration_by_str(
    underlying_book: &std::sync::Arc<option_chain_orderbook::orderbook::UnderlyingOrderBook>,
    exp_str: &str,
) -> Option<ExpirationDate> {
    for entry in underlying_book.expirations().iter() {
        if format_expiration(entry.key()) == exp_str {
            return Some(*entry.key());
        }
    }
    None
}

/// Parses expiration string to ExpirationDate.
fn parse_expiration(exp_str: &str) -> Result<ExpirationDate, ApiError> {
    // Try parsing as days first
    if let Ok(days) = exp_str.parse::<i32>() {
        use optionstratlib::prelude::pos_or_panic;
        return Ok(ExpirationDate::Days(pos_or_panic!(days as f64)));
    }

    // Try parsing as YYYYMMDD format
    if exp_str.len() == 8
        && let (Ok(year), Ok(month), Ok(day)) = (
            exp_str[0..4].parse::<i32>(),
            exp_str[4..6].parse::<u32>(),
            exp_str[6..8].parse::<u32>(),
        )
    {
        use chrono::{NaiveDate, Utc};
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            let datetime = date.and_hms_opt(16, 0, 0).unwrap();
            let utc_datetime = chrono::DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc);
            return Ok(ExpirationDate::DateTime(utc_datetime));
        }
    }

    Err(ApiError::InvalidRequest(format!(
        "Invalid expiration format: {}. Use days (e.g., '30') or YYYYMMDD (e.g., '20240329')",
        exp_str
    )))
}

// ============================================================================
// Health Check
// ============================================================================

/// Health check endpoint.
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    ),
    tag = "Health"
)]
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// ============================================================================
// Global Statistics
// ============================================================================

/// Get global statistics.
#[utoipa::path(
    get,
    path = "/api/v1/stats",
    responses(
        (status = 200, description = "Global statistics", body = GlobalStatsResponse)
    ),
    tag = "Statistics"
)]
pub async fn get_global_stats(State(state): State<Arc<AppState>>) -> Json<GlobalStatsResponse> {
    let stats = state.manager.stats();
    Json(GlobalStatsResponse {
        underlying_count: stats.underlying_count,
        total_expirations: stats.total_expirations,
        total_strikes: stats.total_strikes,
        total_orders: stats.total_orders,
    })
}

// ============================================================================
// Underlying Management
// ============================================================================

/// List all underlyings.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings",
    responses(
        (status = 200, description = "List of underlyings", body = UnderlyingsListResponse)
    ),
    tag = "Underlyings"
)]
pub async fn list_underlyings(State(state): State<Arc<AppState>>) -> Json<UnderlyingsListResponse> {
    let underlyings = state.manager.underlying_symbols();
    Json(UnderlyingsListResponse { underlyings })
}

/// Get or create an underlying.
#[utoipa::path(
    post,
    path = "/api/v1/underlyings/{underlying}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol")
    ),
    responses(
        (status = 200, description = "Underlying created or retrieved", body = UnderlyingSummary)
    ),
    tag = "Underlyings"
)]
pub async fn create_underlying(
    State(state): State<Arc<AppState>>,
    Path(underlying): Path<String>,
) -> Json<UnderlyingSummary> {
    let book = state.manager.get_or_create(&underlying);

    Json(UnderlyingSummary {
        symbol: book.underlying().to_string(),
        expiration_count: book.expiration_count(),
        total_strike_count: book.total_strike_count(),
        total_order_count: book.total_order_count(),
    })
}

/// Get underlying details.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol")
    ),
    responses(
        (status = 200, description = "Underlying details", body = UnderlyingSummary),
        (status = 404, description = "Underlying not found")
    ),
    tag = "Underlyings"
)]
pub async fn get_underlying(
    State(state): State<Arc<AppState>>,
    Path(underlying): Path<String>,
) -> Result<Json<UnderlyingSummary>, ApiError> {
    let book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    Ok(Json(UnderlyingSummary {
        symbol: book.underlying().to_string(),
        expiration_count: book.expiration_count(),
        total_strike_count: book.total_strike_count(),
        total_order_count: book.total_order_count(),
    }))
}

/// Delete an underlying.
#[utoipa::path(
    delete,
    path = "/api/v1/underlyings/{underlying}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol")
    ),
    responses(
        (status = 200, description = "Underlying deleted"),
        (status = 404, description = "Underlying not found")
    ),
    tag = "Underlyings"
)]
pub async fn delete_underlying(
    State(state): State<Arc<AppState>>,
    Path(underlying): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if state.manager.remove(&underlying) {
        Ok(Json(serde_json::json!({
            "message": format!("Underlying {} deleted", underlying)
        })))
    } else {
        Err(ApiError::UnderlyingNotFound(underlying))
    }
}

// ============================================================================
// Expiration Management
// ============================================================================

/// List expirations for an underlying.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations",
    params(
        ("underlying" = String, Path, description = "Underlying symbol")
    ),
    responses(
        (status = 200, description = "List of expirations", body = ExpirationsListResponse),
        (status = 404, description = "Underlying not found")
    ),
    tag = "Expirations"
)]
pub async fn list_expirations(
    State(state): State<Arc<AppState>>,
    Path(underlying): Path<String>,
) -> Result<Json<ExpirationsListResponse>, ApiError> {
    let book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying))?;

    let expirations: Vec<String> = book
        .expirations()
        .iter()
        .map(|e| format_expiration(e.key()))
        .collect();

    Ok(Json(ExpirationsListResponse { expirations }))
}

/// Create or get an expiration.
#[utoipa::path(
    post,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date (YYYYMMDD or days)")
    ),
    responses(
        (status = 200, description = "Expiration created or retrieved", body = ExpirationSummary)
    ),
    tag = "Expirations"
)]
pub async fn create_expiration(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str)): Path<(String, String)>,
) -> Result<Json<ExpirationSummary>, ApiError> {
    let expiration = parse_expiration(&exp_str)?;

    let underlying_book = state.manager.get_or_create(&underlying);
    let exp_book = underlying_book.get_or_create_expiration(expiration);

    Ok(Json(ExpirationSummary {
        expiration: exp_book.expiration().to_string(),
        strike_count: exp_book.strike_count(),
        total_order_count: exp_book.chain().total_order_count(),
    }))
}

/// Get expiration details.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date (YYYYMMDD or days)")
    ),
    responses(
        (status = 200, description = "Expiration details", body = ExpirationSummary),
        (status = 404, description = "Not found")
    ),
    tag = "Expirations"
)]
pub async fn get_expiration(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str)): Path<(String, String)>,
) -> Result<Json<ExpirationSummary>, ApiError> {
    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    Ok(Json(ExpirationSummary {
        expiration: format_expiration(exp_book.expiration()),
        strike_count: exp_book.strike_count(),
        total_order_count: exp_book.chain().total_order_count(),
    }))
}

// ============================================================================
// Strike Management
// ============================================================================

/// List strikes for an expiration.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date")
    ),
    responses(
        (status = 200, description = "List of strikes", body = StrikesListResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Strikes"
)]
pub async fn list_strikes(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str)): Path<(String, String)>,
) -> Result<Json<StrikesListResponse>, ApiError> {
    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    let strikes = exp_book.strike_prices();

    Ok(Json(StrikesListResponse { strikes }))
}

/// Create or get a strike.
#[utoipa::path(
    post,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price")
    ),
    responses(
        (status = 200, description = "Strike created or retrieved", body = StrikeSummary)
    ),
    tag = "Strikes"
)]
pub async fn create_strike(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike)): Path<(String, String, u64)>,
) -> Result<Json<StrikeSummary>, ApiError> {
    let expiration = parse_expiration(&exp_str)?;

    let underlying_book = state.manager.get_or_create(&underlying);
    let exp_book = underlying_book.get_or_create_expiration(expiration);
    let strike_book = exp_book.get_or_create_strike(strike);

    Ok(Json(StrikeSummary {
        strike: strike_book.strike(),
        call_order_count: strike_book.call().order_count(),
        put_order_count: strike_book.put().order_count(),
        call_quote: quote_to_response(&strike_book.call_quote()),
        put_quote: quote_to_response(&strike_book.put_quote()),
    }))
}

/// Get strike details.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price")
    ),
    responses(
        (status = 200, description = "Strike details", body = StrikeSummary),
        (status = 404, description = "Not found")
    ),
    tag = "Strikes"
)]
pub async fn get_strike(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike)): Path<(String, String, u64)>,
) -> Result<Json<StrikeSummary>, ApiError> {
    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    let strike_book = exp_book
        .get_strike(strike)
        .map_err(|_| ApiError::StrikeNotFound(strike))?;

    Ok(Json(StrikeSummary {
        strike: strike_book.strike(),
        call_order_count: strike_book.call().order_count(),
        put_order_count: strike_book.put().order_count(),
        call_quote: quote_to_response(&strike_book.call_quote()),
        put_quote: quote_to_response(&strike_book.put_quote()),
    }))
}

// ============================================================================
// Option Order Book Management
// ============================================================================

/// Get option order book snapshot.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'")
    ),
    responses(
        (status = 200, description = "Option order book snapshot", body = OrderBookSnapshotResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn get_option_book(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
) -> Result<Json<OrderBookSnapshotResponse>, ApiError> {
    let option_style = parse_option_style(&style)?;

    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    let strike_book = exp_book
        .get_strike(strike)
        .map_err(|_| ApiError::StrikeNotFound(strike))?;

    let option_book = strike_book.get(option_style);
    let quote = option_book.best_quote();

    Ok(Json(OrderBookSnapshotResponse {
        symbol: option_book.symbol().to_string(),
        total_bid_depth: option_book.total_bid_depth(),
        total_ask_depth: option_book.total_ask_depth(),
        bid_level_count: option_book.bid_level_count(),
        ask_level_count: option_book.ask_level_count(),
        order_count: option_book.order_count(),
        quote: quote_to_response(&quote),
    }))
}

/// Add order to option book.
#[utoipa::path(
    post,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'")
    ),
    request_body = AddOrderRequest,
    responses(
        (status = 200, description = "Order added", body = AddOrderResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn add_order(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
    Json(body): Json<AddOrderRequest>,
) -> Result<Json<AddOrderResponse>, ApiError> {
    let expiration = parse_expiration(&exp_str)?;
    let option_style = parse_option_style(&style)?;
    let side = order_side_to_side(body.side);

    let underlying_book = state.manager.get_or_create(&underlying);
    let exp_book = underlying_book.get_or_create_expiration(expiration);
    let strike_book = exp_book.get_or_create_strike(strike);
    let option_book = strike_book.get(option_style);

    let order_id = OrderId::new();
    option_book
        .add_limit_order(order_id, side, body.price, body.quantity)
        .map_err(|e| ApiError::OrderBook(e.to_string()))?;

    Ok(Json(AddOrderResponse {
        order_id: order_id.to_string(),
        message: "Order added successfully".to_string(),
    }))
}

/// Cancel order from option book.
#[utoipa::path(
    delete,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders/{order_id}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'"),
        ("order_id" = String, Path, description = "Order ID to cancel")
    ),
    responses(
        (status = 200, description = "Order canceled", body = CancelOrderResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn cancel_order(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style, order_id_str)): Path<(
        String,
        String,
        u64,
        String,
        String,
    )>,
) -> Result<Json<CancelOrderResponse>, ApiError> {
    let option_style = parse_option_style(&style)?;

    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    let strike_book = exp_book
        .get_strike(strike)
        .map_err(|_| ApiError::StrikeNotFound(strike))?;

    let option_book = strike_book.get(option_style);

    // Parse order ID
    let order_id: OrderId = order_id_str
        .parse()
        .map_err(|_| ApiError::InvalidRequest(format!("Invalid order ID: {}", order_id_str)))?;

    let success = option_book
        .cancel_order(order_id)
        .map_err(|e| ApiError::OrderBook(e.to_string()))?;

    Ok(Json(CancelOrderResponse {
        success,
        message: if success {
            "Order canceled successfully".to_string()
        } else {
            "Order not found".to_string()
        },
    }))
}

/// Get option quote.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/quote",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'")
    ),
    responses(
        (status = 200, description = "Option quote", body = QuoteResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn get_option_quote(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
) -> Result<Json<QuoteResponse>, ApiError> {
    let option_style = parse_option_style(&style)?;

    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    let strike_book = exp_book
        .get_strike(strike)
        .map_err(|_| ApiError::StrikeNotFound(strike))?;

    let option_book = strike_book.get(option_style);
    let quote = option_book.best_quote();

    Ok(Json(quote_to_response(&quote)))
}

// ============================================================================
// Enriched Snapshot
// ============================================================================

/// Get enriched order book snapshot with configurable depth.
///
/// Returns a snapshot of the order book with pre-calculated metrics including
/// mid price, spread, depth totals, imbalance, and VWAP.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/snapshot",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'"),
        ("depth" = Option<String>, Query, description = "Depth: 'top' (default), '10', '20', or 'full'")
    ),
    responses(
        (status = 200, description = "Enriched order book snapshot", body = EnrichedSnapshotResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn get_option_snapshot(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Json<EnrichedSnapshotResponse>, ApiError> {
    let option_style = parse_option_style(&style)?;

    // Parse depth parameter
    let depth = query
        .depth
        .as_deref()
        .unwrap_or("top")
        .parse::<SnapshotDepth>()
        .map_err(ApiError::InvalidRequest)?;

    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str))?;

    let strike_book = exp_book
        .get_strike(strike)
        .map_err(|_| ApiError::StrikeNotFound(strike))?;

    let option_book = strike_book.get(option_style);

    // Get enriched snapshot from the inner orderbook
    let enriched = option_book.inner().enriched_snapshot(depth.to_usize());

    // Build symbol string
    let style_str = match option_style {
        OptionStyle::Call => "C",
        OptionStyle::Put => "P",
    };
    let symbol = format!(
        "{}_{}_{}_{}",
        underlying,
        format_expiration(&expiration),
        strike,
        style_str
    );

    // Convert price levels
    let bids: Vec<PriceLevelInfo> = enriched
        .bids
        .iter()
        .map(|level| PriceLevelInfo {
            price: level.price,
            quantity: level.visible_quantity,
            order_count: level.order_count,
        })
        .collect();

    let asks: Vec<PriceLevelInfo> = enriched
        .asks
        .iter()
        .map(|level| PriceLevelInfo {
            price: level.price,
            quantity: level.visible_quantity,
            order_count: level.order_count,
        })
        .collect();

    let stats = SnapshotStats {
        mid_price: enriched.mid_price,
        spread_bps: enriched.spread_bps,
        bid_depth_total: enriched.bid_depth_total,
        ask_depth_total: enriched.ask_depth_total,
        imbalance: enriched.order_book_imbalance,
        vwap_bid: enriched.vwap_bid,
        vwap_ask: enriched.vwap_ask,
    };

    Ok(Json(EnrichedSnapshotResponse {
        symbol,
        sequence: enriched.timestamp,
        timestamp_ms: enriched.timestamp,
        bids,
        asks,
        stats,
    }))
}

// ============================================================================
// Market Order Execution
// ============================================================================

/// Submit a market order to option book.
///
/// Market orders execute immediately against the best available prices in the
/// orderbook. If there is insufficient liquidity, the order will be partially
/// filled or rejected.
#[utoipa::path(
    post,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders/market",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'")
    ),
    request_body = MarketOrderRequest,
    responses(
        (status = 200, description = "Market order executed", body = MarketOrderResponse),
        (status = 400, description = "Invalid request or insufficient liquidity"),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn submit_market_order(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
    Json(body): Json<MarketOrderRequest>,
) -> Result<Json<MarketOrderResponse>, ApiError> {
    if body.quantity == 0 {
        return Err(ApiError::InvalidRequest(
            "quantity must be greater than zero".to_string(),
        ));
    }

    let expiration = parse_expiration(&exp_str)?;
    let option_style = parse_option_style(&style)?;
    let side = order_side_to_side(body.side);

    let underlying_book = state.manager.get_or_create(&underlying);
    let exp_book = underlying_book.get_or_create_expiration(expiration);
    let strike_book = exp_book.get_or_create_strike(strike);
    let option_book = strike_book.get(option_style);

    let order_id = OrderId::new();

    match option_book
        .inner()
        .submit_market_order(order_id, body.quantity, side)
    {
        Ok(match_result) => {
            let filled_quantity = match_result.executed_quantity();
            let remaining_quantity = match_result.remaining_quantity;
            let average_price = match_result.average_price();

            let fills: Vec<FillInfo> = match_result
                .transactions
                .as_vec()
                .iter()
                .map(|t| FillInfo {
                    price: t.price,
                    quantity: t.quantity,
                })
                .collect();

            let status = if match_result.is_complete {
                MarketOrderStatus::Filled
            } else if filled_quantity > 0 {
                MarketOrderStatus::Partial
            } else {
                MarketOrderStatus::Rejected
            };

            Ok(Json(MarketOrderResponse {
                order_id: order_id.to_string(),
                status,
                filled_quantity,
                remaining_quantity,
                average_price,
                fills,
            }))
        }
        Err(e) => Err(ApiError::OrderBook(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn create_test_state() -> Arc<AppState> {
        Arc::new(AppState::new())
    }

    #[tokio::test]
    async fn test_market_order_full_fill() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("TEST");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(100);
        let option_book = strike_book.get(OptionStyle::Call);

        // Add a sell order (ask) at price 150 with quantity 100
        let sell_order_id = OrderId::new();
        option_book
            .add_limit_order(sell_order_id, Side::Sell, 150, 100)
            .unwrap();

        // Submit market buy order for 50
        let request = MarketOrderRequest {
            side: OrderSide::Buy,
            quantity: 50,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
            Json(request),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.status, MarketOrderStatus::Filled);
        assert_eq!(response.filled_quantity, 50);
        assert_eq!(response.remaining_quantity, 0);
        assert!(response.average_price.is_some());
        assert_eq!(response.average_price.unwrap(), 150.0);
        assert_eq!(response.fills.len(), 1);
        assert_eq!(response.fills[0].price, 150);
        assert_eq!(response.fills[0].quantity, 50);
    }

    #[tokio::test]
    async fn test_market_order_insufficient_liquidity() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("TEST");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(100);
        let option_book = strike_book.get(OptionStyle::Call);

        // Add a sell order (ask) at price 150 with quantity 30
        let sell_order_id = OrderId::new();
        option_book
            .add_limit_order(sell_order_id, Side::Sell, 150, 30)
            .unwrap();

        // Submit market buy order for 50 (only 30 available)
        let request = MarketOrderRequest {
            side: OrderSide::Buy,
            quantity: 50,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
            Json(request),
        )
        .await;

        // orderbook-rs returns a partial fill for insufficient liquidity
        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.status, MarketOrderStatus::Partial);
        assert_eq!(response.filled_quantity, 30);
        assert_eq!(response.remaining_quantity, 20);
    }

    #[tokio::test]
    async fn test_market_order_zero_quantity() {
        let state = create_test_state();

        let request = MarketOrderRequest {
            side: OrderSide::Buy,
            quantity: 0,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
            Json(request),
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::InvalidRequest(msg) => {
                assert!(msg.contains("quantity must be greater than zero"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_market_order_no_liquidity() {
        let state = create_test_state();

        // Create underlying, expiration, strike (no orders)
        let underlying_book = state.manager.get_or_create("TEST");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let _strike_book = exp_book.get_or_create_strike(100);

        // Submit market buy order with no liquidity
        let request = MarketOrderRequest {
            side: OrderSide::Buy,
            quantity: 50,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
            Json(request),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_market_order_invalid_option_style() {
        let state = create_test_state();

        let request = MarketOrderRequest {
            side: OrderSide::Buy,
            quantity: 50,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "invalid".to_string(),
            )),
            Json(request),
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::InvalidRequest(msg) => {
                assert!(msg.contains("Invalid option style"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_market_order_sell_side() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("TEST");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(100);
        let option_book = strike_book.get(OptionStyle::Put);

        // Add a buy order (bid) at price 120 with quantity 100
        let buy_order_id = OrderId::new();
        option_book
            .add_limit_order(buy_order_id, Side::Buy, 120, 100)
            .unwrap();

        // Submit market sell order for 50
        let request = MarketOrderRequest {
            side: OrderSide::Sell,
            quantity: 50,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "put".to_string(),
            )),
            Json(request),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.status, MarketOrderStatus::Filled);
        assert_eq!(response.filled_quantity, 50);
        assert_eq!(response.remaining_quantity, 0);
        assert_eq!(response.average_price.unwrap(), 120.0);
    }

    #[tokio::test]
    async fn test_market_order_multiple_fills() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("TEST");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(100);
        let option_book = strike_book.get(OptionStyle::Call);

        // Add multiple sell orders at different prices
        let sell_order_id1 = OrderId::new();
        option_book
            .add_limit_order(sell_order_id1, Side::Sell, 150, 30)
            .unwrap();

        let sell_order_id2 = OrderId::new();
        option_book
            .add_limit_order(sell_order_id2, Side::Sell, 155, 40)
            .unwrap();

        let sell_order_id3 = OrderId::new();
        option_book
            .add_limit_order(sell_order_id3, Side::Sell, 160, 50)
            .unwrap();

        // Submit market buy order for 60 (should fill 30@150 + 30@155)
        let request = MarketOrderRequest {
            side: OrderSide::Buy,
            quantity: 60,
        };

        let result = submit_market_order(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
            Json(request),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.status, MarketOrderStatus::Filled);
        assert_eq!(response.filled_quantity, 60);
        assert_eq!(response.remaining_quantity, 0);
        assert_eq!(response.fills.len(), 2);
        // First fill at best price
        assert_eq!(response.fills[0].price, 150);
        assert_eq!(response.fills[0].quantity, 30);
        // Second fill at next best price
        assert_eq!(response.fills[1].price, 155);
        assert_eq!(response.fills[1].quantity, 30);
        // Average price should be (30*150 + 30*155) / 60 = 152.5
        assert!((response.average_price.unwrap() - 152.5).abs() < 0.01);
    }

    // ========================================================================
    // Snapshot Tests
    // ========================================================================

    #[tokio::test]
    async fn test_snapshot_depth_parsing() {
        use std::str::FromStr;

        assert_eq!(SnapshotDepth::from_str("top").unwrap(), SnapshotDepth::Top);
        assert_eq!(SnapshotDepth::from_str("1").unwrap(), SnapshotDepth::Top);
        assert_eq!(
            SnapshotDepth::from_str("10").unwrap(),
            SnapshotDepth::Levels(10)
        );
        assert_eq!(
            SnapshotDepth::from_str("20").unwrap(),
            SnapshotDepth::Levels(20)
        );
        assert_eq!(
            SnapshotDepth::from_str("full").unwrap(),
            SnapshotDepth::Full
        );
        assert_eq!(SnapshotDepth::from_str("all").unwrap(), SnapshotDepth::Full);
        assert!(SnapshotDepth::from_str("invalid").is_err());
    }

    #[tokio::test]
    async fn test_snapshot_depth_to_usize() {
        assert_eq!(SnapshotDepth::Top.to_usize(), 1);
        assert_eq!(SnapshotDepth::Levels(10).to_usize(), 10);
        assert_eq!(SnapshotDepth::Full.to_usize(), usize::MAX);
    }

    #[tokio::test]
    async fn test_snapshot_empty_book() {
        let state = create_test_state();

        // Create underlying, expiration, strike (no orders)
        let underlying_book = state.manager.get_or_create("SNAP");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let _strike_book = exp_book.get_or_create_strike(100);

        // Use the formatted expiration string that matches what find_expiration_by_str expects
        let exp_str = format_expiration(&expiration);

        let result = get_option_snapshot(
            State(state.clone()),
            Path(("SNAP".to_string(), exp_str, 100u64, "call".to_string())),
            Query(SnapshotQuery { depth: None }),
        )
        .await;

        assert!(result.is_ok(), "Error: {:?}", result.err());
        let response = result.unwrap().0;
        assert!(response.bids.is_empty());
        assert!(response.asks.is_empty());
        assert!(response.stats.mid_price.is_none());
        assert!(response.stats.spread_bps.is_none());
        assert_eq!(response.stats.bid_depth_total, 0);
        assert_eq!(response.stats.ask_depth_total, 0);
    }

    #[tokio::test]
    async fn test_snapshot_with_orders() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("SNAP2");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(100);
        let option_book = strike_book.get(OptionStyle::Call);

        // Add bid and ask orders
        option_book
            .add_limit_order(OrderId::new(), Side::Buy, 100, 50)
            .unwrap();
        option_book
            .add_limit_order(OrderId::new(), Side::Sell, 110, 30)
            .unwrap();

        // Use the formatted expiration string that matches what find_expiration_by_str expects
        let exp_str = format_expiration(&expiration);

        let result = get_option_snapshot(
            State(state.clone()),
            Path(("SNAP2".to_string(), exp_str, 100u64, "call".to_string())),
            Query(SnapshotQuery {
                depth: Some("full".to_string()),
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Check bids
        assert_eq!(response.bids.len(), 1);
        assert_eq!(response.bids[0].price, 100);
        assert_eq!(response.bids[0].quantity, 50);
        assert_eq!(response.bids[0].order_count, 1);

        // Check asks
        assert_eq!(response.asks.len(), 1);
        assert_eq!(response.asks[0].price, 110);
        assert_eq!(response.asks[0].quantity, 30);
        assert_eq!(response.asks[0].order_count, 1);

        // Check stats
        assert!(response.stats.mid_price.is_some());
        let mid = response.stats.mid_price.unwrap();
        assert!((mid - 105.0).abs() < 0.01); // (100 + 110) / 2 = 105

        assert!(response.stats.spread_bps.is_some());
        assert_eq!(response.stats.bid_depth_total, 50);
        assert_eq!(response.stats.ask_depth_total, 30);
    }

    #[tokio::test]
    async fn test_snapshot_multiple_levels() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("SNAP3");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(100);
        let option_book = strike_book.get(OptionStyle::Call);

        // Add multiple bid levels
        option_book
            .add_limit_order(OrderId::new(), Side::Buy, 100, 50)
            .unwrap();
        option_book
            .add_limit_order(OrderId::new(), Side::Buy, 99, 40)
            .unwrap();
        option_book
            .add_limit_order(OrderId::new(), Side::Buy, 98, 30)
            .unwrap();

        // Add multiple ask levels
        option_book
            .add_limit_order(OrderId::new(), Side::Sell, 110, 25)
            .unwrap();
        option_book
            .add_limit_order(OrderId::new(), Side::Sell, 111, 35)
            .unwrap();

        // Use the formatted expiration string that matches what find_expiration_by_str expects
        let exp_str = format_expiration(&expiration);

        // Test with depth=2
        let result = get_option_snapshot(
            State(state.clone()),
            Path(("SNAP3".to_string(), exp_str, 100u64, "call".to_string())),
            Query(SnapshotQuery {
                depth: Some("2".to_string()),
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Should have exactly 2 levels per side (3 bids added, 2 asks added, depth=2)
        assert_eq!(response.bids.len(), 2);
        assert_eq!(response.asks.len(), 2);
    }
}

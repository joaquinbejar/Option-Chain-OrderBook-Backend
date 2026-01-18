//! API request handlers.

use crate::error::ApiError;
use crate::models::{
    AddOrderRequest, AddOrderResponse, CancelOrderResponse, ExpirationSummary,
    ExpirationsListResponse, GlobalStatsResponse, HealthResponse, LastTradeResponse,
    OrderBookSnapshotResponse, QuoteResponse, StrikeSummary, StrikesListResponse,
    UnderlyingSummary, UnderlyingsListResponse,
};
use crate::state::AppState;
use axum::Json;
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

/// Parses side string to Side enum.
fn parse_side(side: &str) -> Result<Side, ApiError> {
    match side.to_lowercase().as_str() {
        "buy" | "bid" => Ok(Side::Buy),
        "sell" | "ask" => Ok(Side::Sell),
        _ => Err(ApiError::InvalidRequest(format!(
            "Invalid side: {}. Use 'buy' or 'sell'",
            side
        ))),
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
    let side = parse_side(&body.side)?;

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

/// Get last trade information for an option.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/last-trade",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'")
    ),
    responses(
        (status = 200, description = "Last trade information", body = LastTradeResponse),
        (status = 404, description = "No trade found")
    ),
    tag = "Options"
)]
pub async fn get_last_trade(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
) -> Result<Json<LastTradeResponse>, ApiError> {
    // Validate option style
    parse_option_style(&style)?;

    // Construct symbol key for lookup
    let symbol = format!("{}-{}-{}-{}", underlying, exp_str, strike, style);

    // Look up last trade information
    if let Some(trade_info) = state.last_trades.get(&symbol) {
        Ok(Json(LastTradeResponse {
            symbol: trade_info.symbol.clone(),
            price: trade_info.price,
            quantity: trade_info.quantity,
            side: trade_info.side.clone(),
            timestamp_ms: trade_info.timestamp_ms,
            trade_id: trade_info.trade_id.clone(),
        }))
    } else {
        Err(ApiError::NotFound(format!(
            "No trade found for symbol: {}",
            symbol
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, LastTradeInfo};
    use axum::extract::Path;
    use dashmap::DashMap;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn create_test_state() -> Arc<AppState> {
        let last_trades = Arc::new(DashMap::new());
        let (trade_tx, _) = broadcast::channel(1000);

        // Add some test trade data
        let trade_info = LastTradeInfo {
            symbol: "AAPL-20250120-150-call".to_string(),
            price: 50000, // $5.00 in cents
            quantity: 100,
            side: "buy".to_string(),
            timestamp_ms: 1642694400000, // Jan 20, 2022 12:00:00 UTC
            trade_id: "trade_123".to_string(),
        };
        last_trades.insert("AAPL-20250120-150-call".to_string(), trade_info);

        Arc::new(AppState {
            manager: Arc::new(option_chain_orderbook::orderbook::UnderlyingOrderBookManager::new()),
            db: None,
            market_maker: Arc::new(crate::market_maker::MarketMakerEngine::new(
                Arc::new(option_chain_orderbook::orderbook::UnderlyingOrderBookManager::new()),
                None,
            )),
            price_simulator: None,
            config: None,
            last_trades,
            trade_tx,
        })
    }

    /// Creates a test application state with sample trade data for integration tests.
    fn create_integration_test_state() -> Arc<AppState> {
        let last_trades = Arc::new(DashMap::new());
        let (trade_tx, _) = broadcast::channel(1000);

        // Add sample trade data for different options
        let trades = vec![
            LastTradeInfo {
                symbol: "SPY-20250117-450-call".to_string(),
                price: 2500, // $2.50 in cents
                quantity: 200,
                side: "buy".to_string(),
                timestamp_ms: 1642694400000,
                trade_id: "trade_spy_call_001".to_string(),
            },
            LastTradeInfo {
                symbol: "SPY-20250117-450-put".to_string(),
                price: 1800, // $1.80 in cents
                quantity: 150,
                side: "sell".to_string(),
                timestamp_ms: 1642694460000,
                trade_id: "trade_spy_put_001".to_string(),
            },
            LastTradeInfo {
                symbol: "QQQ-20250121-350-call".to_string(),
                price: 4200, // $4.20 in cents
                quantity: 100,
                side: "buy".to_string(),
                timestamp_ms: 1642694520000,
                trade_id: "trade_qqq_call_001".to_string(),
            },
            LastTradeInfo {
                symbol: "QQQ-20250121-350-put".to_string(),
                price: 3800, // $3.80 in cents
                quantity: 75,
                side: "sell".to_string(),
                timestamp_ms: 1642694580000,
                trade_id: "trade_qqq_put_001".to_string(),
            },
        ];

        for trade in trades {
            last_trades.insert(trade.symbol.clone(), trade);
        }

        Arc::new(AppState {
            manager: Arc::new(option_chain_orderbook::orderbook::UnderlyingOrderBookManager::new()),
            db: None,
            market_maker: Arc::new(crate::market_maker::MarketMakerEngine::new(
                Arc::new(option_chain_orderbook::orderbook::UnderlyingOrderBookManager::new()),
                None,
            )),
            price_simulator: None,
            config: None,
            last_trades,
            trade_tx,
        })
    }

    #[tokio::test]
    async fn test_get_last_trade_success() {
        let state = create_test_state();
        let path_params = (
            "AAPL".to_string(),
            "20250120".to_string(),
            150u64,
            "call".to_string(),
        );

        let result = get_last_trade(axum::extract::State(state), Path(path_params)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.symbol, "AAPL-20250120-150-call");
        assert_eq!(response.price, 50000);
        assert_eq!(response.quantity, 100);
        assert_eq!(response.side, "buy");
        assert_eq!(response.timestamp_ms, 1642694400000);
        assert_eq!(response.trade_id, "trade_123");
    }

    #[tokio::test]
    async fn test_get_last_trade_not_found() {
        let state = create_test_state();
        let path_params = (
            "AAPL".to_string(),
            "20250121".to_string(),
            155u64,
            "put".to_string(),
        );

        let result = get_last_trade(axum::extract::State(state), Path(path_params)).await;

        assert!(result.is_err());
        if let Err(ApiError::NotFound(msg)) = result {
            assert!(msg.contains("No trade found for symbol: AAPL-20250121-155-put"));
        } else {
            panic!("Expected NotFound error");
        }
    }

    #[tokio::test]
    async fn test_get_last_trade_invalid_option_style() {
        let state = create_test_state();
        let path_params = (
            "AAPL".to_string(),
            "20250120".to_string(),
            150u64,
            "invalid".to_string(),
        );

        let result = get_last_trade(axum::extract::State(state), Path(path_params)).await;

        assert!(result.is_err());
        // Should fail due to invalid option style
    }

    #[tokio::test]
    async fn test_get_last_trade_put_option() {
        let state = create_test_state();

        // Add a put option trade
        let put_trade_info = LastTradeInfo {
            symbol: "AAPL-20250120-150-put".to_string(),
            price: 30000, // $3.00 in cents
            quantity: 50,
            side: "sell".to_string(),
            timestamp_ms: 1642694460000, // Jan 20, 2022 12:01:00 UTC
            trade_id: "trade_456".to_string(),
        };
        state
            .last_trades
            .insert("AAPL-20250120-150-put".to_string(), put_trade_info);

        let path_params = (
            "AAPL".to_string(),
            "20250120".to_string(),
            150u64,
            "put".to_string(),
        );

        let result = get_last_trade(axum::extract::State(state), Path(path_params)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.symbol, "AAPL-20250120-150-put");
        assert_eq!(response.price, 30000);
        assert_eq!(response.quantity, 50);
        assert_eq!(response.side, "sell");
        assert_eq!(response.timestamp_ms, 1642694460000);
        assert_eq!(response.trade_id, "trade_456");
    }

    // Integration tests moved from tests/last_trade_integration_tests.rs
    #[tokio::test]
    async fn test_last_trade_endpoint_integration() {
        let state = create_integration_test_state();

        // Test 1: Get existing call option trade
        let path_params = (
            "SPY".to_string(),
            "20250117".to_string(),
            450u64,
            "call".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state.clone()), Path(path_params)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.symbol, "SPY-20250117-450-call");
        assert_eq!(response.price, 2500);
        assert_eq!(response.quantity, 200);
        assert_eq!(response.side, "buy");
        assert_eq!(response.trade_id, "trade_spy_call_001");

        // Test 2: Get existing put option trade
        let path_params = (
            "SPY".to_string(),
            "20250117".to_string(),
            450u64,
            "put".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state.clone()), Path(path_params)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.symbol, "SPY-20250117-450-put");
        assert_eq!(response.price, 1800);
        assert_eq!(response.quantity, 150);
        assert_eq!(response.side, "sell");
        assert_eq!(response.trade_id, "trade_spy_put_001");

        // Test 3: Get QQQ call option trade
        let path_params = (
            "QQQ".to_string(),
            "20250121".to_string(),
            350u64,
            "call".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state.clone()), Path(path_params)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.symbol, "QQQ-20250121-350-call");
        assert_eq!(response.price, 4200);
        assert_eq!(response.quantity, 100);
        assert_eq!(response.side, "buy");
        assert_eq!(response.trade_id, "trade_qqq_call_001");

        // Test 4: Non-existent trade should return 404
        let path_params = (
            "SPY".to_string(),
            "20250117".to_string(),
            455u64,
            "call".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state), Path(path_params)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_last_trade_symbol_formatting() {
        let state = create_integration_test_state();

        // Test that the symbol is correctly formatted as: underlying-expiration-strike-style
        let test_cases = vec![
            ("SPY", "20250117", 450, "call", "SPY-20250117-450-call"),
            ("QQQ", "20250121", 350, "put", "QQQ-20250121-350-put"),
        ];

        for (underlying, expiration, strike, style, expected_symbol) in test_cases {
            let path_params = (
                underlying.to_string(),
                expiration.to_string(),
                strike,
                style.to_string(),
            );
            let result =
                get_last_trade(axum::extract::State(state.clone()), Path(path_params)).await;

            assert!(result.is_ok(), "Should succeed for {}", expected_symbol);
            let response = result.unwrap();
            assert_eq!(
                response.symbol, expected_symbol,
                "Symbol should match expected format"
            );
        }
    }

    #[tokio::test]
    async fn test_last_trade_edge_cases() {
        let state = create_integration_test_state();

        // Test with very high strike price (non-existent)
        let path_params = (
            "SPY".to_string(),
            "20250117".to_string(),
            9999u64,
            "call".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state.clone()), Path(path_params)).await;
        assert!(result.is_err(), "Should fail for non-existent high strike");

        // Test with invalid option style
        let path_params = (
            "SPY".to_string(),
            "20250117".to_string(),
            450u64,
            "invalid".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state.clone()), Path(path_params)).await;
        assert!(result.is_err(), "Should fail for invalid option style");

        // Test with different expiration (non-existent)
        let path_params = (
            "SPY".to_string(),
            "20250118".to_string(),
            450u64,
            "call".to_string(),
        );
        let result = get_last_trade(axum::extract::State(state), Path(path_params)).await;
        assert!(result.is_err(), "Should fail for non-existent expiration");
    }
}

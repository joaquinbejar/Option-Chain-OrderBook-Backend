//! API request handlers.

use crate::error::ApiError;
use crate::models::{
    ATMTermStructurePoint, AddOrderRequest, AddOrderResponse, ApiTimeInForce, BulkCancelRequest,
    BulkCancelResponse, BulkCancelResultItem, BulkOrderItem, BulkOrderRequest, BulkOrderResponse,
    BulkOrderResultItem, BulkOrderStatus, CancelAllQuery, CancelAllResponse, CancelOrderResponse,
    ChainQuery, ChainStrikeRow, EnrichedSnapshotResponse, ExpirationSummary,
    ExpirationsListResponse, FillInfo, GlobalStatsResponse, GreeksData, GreeksResponse,
    HealthResponse, LastTradeResponse, LimitOrderStatus, MarketOrderRequest, MarketOrderResponse,
    MarketOrderStatus, ModifyOrderRequest, ModifyOrderResponse, ModifyOrderStatus, OhlcInterval,
    OhlcQuery, OhlcResponse, OptionChainResponse, OptionQuoteData, OrderBookSnapshotResponse,
    OrderInfo, OrderListQuery, OrderListResponse, OrderSide, OrderStatus, OrderStatusResponse,
    OrderTimeInForce, PositionInfo, PositionQuery, PositionResponse, PositionSummary,
    PositionsListResponse, PriceLevelInfo, QuoteResponse, SnapshotDepth, SnapshotQuery,
    SnapshotStats, StrikeIV, StrikeSummary, StrikesListResponse, UnderlyingSummary,
    UnderlyingsListResponse, VolatilitySurfaceResponse,
};
use crate::state::AppState;
use axum::Json;
use axum::extract::Query;
use axum::extract::{Path, State};
use option_chain_orderbook::orderbook::Quote;
use optionstratlib::{ExpirationDate, OptionStyle};
use orderbook_rs::{OrderId, Side, TimeInForce};
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

/// Get the complete option chain matrix for an expiration.
///
/// Returns all strikes with both call and put quotes in a single response.
/// Supports filtering by strike range.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/chain",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("min_strike" = Option<u64>, Query, description = "Minimum strike price filter"),
        ("max_strike" = Option<u64>, Query, description = "Maximum strike price filter")
    ),
    responses(
        (status = 200, description = "Option chain matrix", body = OptionChainResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Chain"
)]
pub async fn get_option_chain(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str)): Path<(String, String)>,
    Query(query): Query<ChainQuery>,
) -> Result<Json<OptionChainResponse>, ApiError> {
    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str.clone()))?;

    // Get all strikes
    let mut strikes = exp_book.strike_prices();
    strikes.sort();

    // Apply strike range filters
    let filtered_strikes: Vec<u64> = strikes
        .into_iter()
        .filter(|&strike| {
            if let Some(min) = query.min_strike
                && strike < min
            {
                return false;
            }
            if let Some(max) = query.max_strike
                && strike > max
            {
                return false;
            }
            true
        })
        .collect();

    // Get spot price from price simulator if available
    let spot_price = state
        .price_simulator
        .as_ref()
        .and_then(|sim| sim.get_price(&underlying))
        .map(|p| p as u128);

    // Determine ATM strike (closest to spot price)
    let atm_strike = spot_price.and_then(|spot| {
        filtered_strikes
            .iter()
            .min_by_key(|&&strike| {
                let strike_128 = strike as u128;
                strike_128.abs_diff(spot)
            })
            .copied()
    });

    // Build chain data
    let mut chain: Vec<ChainStrikeRow> = Vec::with_capacity(filtered_strikes.len());

    for strike in filtered_strikes {
        if let Ok(strike_book) = exp_book.get_strike(strike) {
            // Get call quote
            let call_quote = strike_book.call_quote();
            let call_data =
                build_option_quote_data(&call_quote, &state, &underlying, &exp_str, strike, "C");

            // Get put quote
            let put_quote = strike_book.put_quote();
            let put_data =
                build_option_quote_data(&put_quote, &state, &underlying, &exp_str, strike, "P");

            chain.push(ChainStrikeRow {
                strike,
                call: call_data,
                put: put_data,
            });
        }
    }

    Ok(Json(OptionChainResponse {
        underlying: underlying.clone(),
        expiration: exp_str,
        spot_price,
        atm_strike,
        chain,
    }))
}

/// Helper function to build OptionQuoteData from a Quote.
fn build_option_quote_data(
    quote: &Quote,
    state: &Arc<AppState>,
    underlying: &str,
    expiration: &str,
    strike: u64,
    style_char: &str,
) -> OptionQuoteData {
    // Build symbol for last trade lookup
    let symbol = format!("{}-{}-{}-{}", underlying, expiration, strike, style_char);

    // Get last trade price if available
    let last_trade = state.last_trades.get(&symbol).map(|entry| entry.price);

    OptionQuoteData {
        bid: quote.bid_price(),
        ask: quote.ask_price(),
        bid_size: quote.bid_size(),
        ask_size: quote.ask_size(),
        last_trade: last_trade.map(|p| p as u128),
        volume: 0,        // Not tracked yet
        open_interest: 0, // Not tracked yet
        delta: None,      // Greeks not calculated yet
        gamma: None,
        theta: None,
        vega: None,
        iv: None,
    }
}

// ============================================================================
// Implied Volatility Surface
// ============================================================================

/// Get the implied volatility surface for an underlying.
///
/// Returns IV data across all strikes and expirations, enabling volatility
/// surface visualization and analysis.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/volatility-surface",
    params(
        ("underlying" = String, Path, description = "Underlying symbol")
    ),
    responses(
        (status = 200, description = "Volatility surface data", body = VolatilitySurfaceResponse),
        (status = 404, description = "Underlying not found")
    ),
    tag = "Volatility"
)]
pub async fn get_volatility_surface(
    State(state): State<Arc<AppState>>,
    Path(underlying): Path<String>,
) -> Result<Json<VolatilitySurfaceResponse>, ApiError> {
    use std::collections::HashMap;

    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    // Get spot price from price simulator
    let spot_price = state
        .price_simulator
        .as_ref()
        .and_then(|sim| sim.get_price(&underlying));

    // Collect all expirations
    let expirations_map = underlying_book.expirations();
    let mut expirations: Vec<String> = Vec::new();
    let mut all_strikes: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    let mut surface: HashMap<String, HashMap<u64, StrikeIV>> = HashMap::new();
    let mut atm_term_structure: Vec<ATMTermStructurePoint> = Vec::new();

    let now = chrono::Utc::now();

    for exp_entry in expirations_map.iter() {
        let exp = exp_entry.key();
        let exp_str = match exp.get_date() {
            Ok(d) => d.format("%Y%m%d").to_string(),
            Err(_) => continue,
        };

        let exp_book = match underlying_book.get_expiration(exp) {
            Ok(book) => book,
            Err(_) => continue,
        };

        // Calculate days to expiry
        let days_to_expiry = match exp.get_date() {
            Ok(d) => (d - now).num_days().max(1) as u64,
            Err(_) => 1,
        };

        let strikes = exp_book.strike_prices();
        let mut exp_surface: HashMap<u64, StrikeIV> = HashMap::new();

        // Find ATM strike for this expiration
        let atm_strike =
            spot_price.and_then(|spot| strikes.iter().min_by_key(|&&s| s.abs_diff(spot)).copied());

        for strike in &strikes {
            all_strikes.insert(*strike);

            let strike_book = match exp_book.get_strike(*strike) {
                Ok(book) => book,
                Err(_) => continue,
            };

            // Get call and put quotes
            let call_quote = strike_book.call_quote();
            let put_quote = strike_book.put_quote();

            // Calculate mid-price for IV estimation
            let call_mid = calculate_mid_price(&call_quote);
            let put_mid = calculate_mid_price(&put_quote);

            // For now, use a simple IV estimation based on moneyness
            // In production, this would use optionstratlib::volatility::calculate_iv
            let call_iv = call_mid.map(|_| estimate_iv(spot_price, *strike, days_to_expiry, true));
            let put_iv = put_mid.map(|_| estimate_iv(spot_price, *strike, days_to_expiry, false));

            exp_surface.insert(*strike, StrikeIV { call_iv, put_iv });
        }

        // Add ATM IV to term structure
        if let Some(atm) = atm_strike
            && let Some(strike_iv) = exp_surface.get(&atm)
        {
            let atm_iv = strike_iv.call_iv.or(strike_iv.put_iv).unwrap_or(0.30);
            atm_term_structure.push(ATMTermStructurePoint {
                expiration: exp_str.clone(),
                days: days_to_expiry,
                iv: atm_iv,
            });
        }

        expirations.push(exp_str.clone());
        surface.insert(exp_str, exp_surface);
    }

    // Sort expirations and term structure by days
    expirations.sort();
    atm_term_structure.sort_by_key(|p| p.days);

    let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;

    Ok(Json(VolatilitySurfaceResponse {
        underlying: underlying.clone(),
        spot_price,
        timestamp_ms,
        expirations,
        strikes: all_strikes.into_iter().collect(),
        surface,
        atm_term_structure,
    }))
}

/// Calculate mid-price from a quote.
fn calculate_mid_price(quote: &Quote) -> Option<u128> {
    match (quote.bid_price(), quote.ask_price()) {
        (Some(bid), Some(ask)) => Some((bid + ask) / 2),
        (Some(bid), None) => Some(bid),
        (None, Some(ask)) => Some(ask),
        (None, None) => None,
    }
}

/// Estimate IV based on moneyness and time to expiry.
/// This is a simplified estimation; in production, use optionstratlib::volatility::calculate_iv.
fn estimate_iv(spot_price: Option<u64>, strike: u64, days_to_expiry: u64, is_call: bool) -> f64 {
    let base_iv = 0.30; // 30% base volatility

    // Adjust for moneyness (volatility smile)
    let moneyness = spot_price.map(|s| strike as f64 / s as f64).unwrap_or(1.0);
    let smile_adjustment = (moneyness - 1.0).abs() * 0.1;

    // Adjust for time (term structure - typically upward sloping)
    let term_adjustment = (days_to_expiry as f64 / 365.0).sqrt() * 0.02;

    // Slight skew for puts (typically higher IV for OTM puts)
    let skew = if !is_call && moneyness < 1.0 {
        0.02
    } else {
        0.0
    };

    base_iv + smile_adjustment + term_adjustment + skew
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

    // Convert API TimeInForce to orderbook-rs TimeInForce
    let tif = match body.time_in_force.unwrap_or_default() {
        ApiTimeInForce::Gtc => TimeInForce::Gtc,
        ApiTimeInForce::Ioc => TimeInForce::Ioc,
        ApiTimeInForce::Fok => TimeInForce::Fok,
        ApiTimeInForce::Gtd => {
            // Parse expire_at timestamp for GTD orders
            let expire_ms = if let Some(expire_str) = &body.expire_at {
                chrono::DateTime::parse_from_rfc3339(expire_str)
                    .map(|dt| dt.timestamp_millis() as u64)
                    .unwrap_or(0)
            } else {
                0 // Default to 0 if no expiration provided
            };
            TimeInForce::Gtd(expire_ms)
        }
    };

    let underlying_book = state.manager.get_or_create(&underlying);
    let exp_book = underlying_book.get_or_create_expiration(expiration);
    let strike_book = exp_book.get_or_create_strike(strike);
    let option_book = strike_book.get(option_style);

    let order_id = OrderId::new();

    // Use add_limit_order_with_tif for TIF support
    match option_book.add_limit_order_with_tif(order_id, side, body.price, body.quantity, tif) {
        Ok(()) => {
            // For GTC orders, the order is accepted and placed in the book
            Ok(Json(AddOrderResponse {
                order_id: order_id.to_string(),
                status: LimitOrderStatus::Accepted,
                filled_quantity: 0,
                remaining_quantity: body.quantity,
                message: format!("Order added successfully with TIF={}", tif),
            }))
        }
        Err(e) => {
            let error_str = e.to_string();
            // Check if it's an IOC/FOK rejection due to insufficient liquidity
            if error_str.contains("InsufficientLiquidity") || error_str.contains("insufficient") {
                Ok(Json(AddOrderResponse {
                    order_id: order_id.to_string(),
                    status: LimitOrderStatus::Rejected,
                    filled_quantity: 0,
                    remaining_quantity: body.quantity,
                    message: format!("Order rejected: {}", error_str),
                }))
            } else {
                Err(ApiError::OrderBook(error_str))
            }
        }
    }
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

/// Modify an existing order's price and/or quantity.
///
/// This implements order modification using cancel-and-replace semantics.
/// The order is canceled and a new order is placed with the updated parameters.
/// This means the order will always lose time priority.
#[utoipa::path(
    patch,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders/{order_id}",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'"),
        ("order_id" = String, Path, description = "Order identifier to modify")
    ),
    request_body = ModifyOrderRequest,
    responses(
        (status = 200, description = "Order modification result", body = ModifyOrderResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Order not found")
    ),
    tag = "Options"
)]
pub async fn modify_order(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style, order_id_str)): Path<(
        String,
        String,
        u64,
        String,
        String,
    )>,
    Json(body): Json<ModifyOrderRequest>,
) -> Result<Json<ModifyOrderResponse>, ApiError> {
    // Validate that at least one field is provided
    if body.price.is_none() && body.quantity.is_none() {
        return Err(ApiError::InvalidRequest(
            "At least one of 'price' or 'quantity' must be provided".to_string(),
        ));
    }

    let option_style = parse_option_style(&style)?;

    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let strike_book = exp_book
        .get_strike(strike)
        .map_err(|_| ApiError::StrikeNotFound(strike))?;

    let option_book = strike_book.get(option_style);

    // Parse order ID
    let order_id: OrderId = order_id_str
        .parse()
        .map_err(|_| ApiError::InvalidRequest(format!("Invalid order ID: {}", order_id_str)))?;

    // Get the existing order from the order book
    let existing_order = option_book
        .inner()
        .get_order(order_id)
        .ok_or_else(|| ApiError::NotFound(format!("Order not found: {}", order_id_str)))?;

    // Get current order parameters
    let current_price = existing_order.price();
    let current_quantity = existing_order.visible_quantity();
    let side = existing_order.side();

    // Determine new values
    let new_price = body.price.unwrap_or(current_price);
    let new_quantity = body.quantity.unwrap_or(current_quantity);

    // Cancel the existing order
    let canceled = option_book
        .cancel_order(order_id)
        .map_err(|e| ApiError::OrderBook(e.to_string()))?;

    if !canceled {
        return Ok(Json(ModifyOrderResponse {
            order_id: order_id_str,
            status: ModifyOrderStatus::Rejected,
            new_price: None,
            new_quantity: None,
            priority_changed: false,
            message: "Failed to cancel existing order for modification".to_string(),
        }));
    }

    // Create a new order with the updated parameters
    let new_order_id = OrderId::new();

    match option_book.add_limit_order(new_order_id, side, new_price, new_quantity) {
        Ok(()) => {
            // Update order info in AppState
            if let Some(order_info) = state.orders.remove(&order_id_str) {
                let mut updated_info = order_info.1;
                updated_info.order_id = new_order_id.to_string();
                updated_info.price = new_price;
                updated_info.remaining_quantity = new_quantity;
                updated_info.updated_at_ms = chrono::Utc::now().timestamp_millis() as u64;
                state.orders.insert(new_order_id.to_string(), updated_info);
            }

            Ok(Json(ModifyOrderResponse {
                order_id: new_order_id.to_string(),
                status: ModifyOrderStatus::Modified,
                new_price: Some(new_price),
                new_quantity: Some(new_quantity),
                priority_changed: true, // Cancel-and-replace always loses priority
                message: "Order modified successfully (cancel-and-replace)".to_string(),
            }))
        }
        Err(e) => {
            // Failed to place new order - the original is already canceled
            Ok(Json(ModifyOrderResponse {
                order_id: order_id_str,
                status: ModifyOrderStatus::Rejected,
                new_price: None,
                new_quantity: None,
                priority_changed: false,
                message: format!("Order canceled but failed to place replacement: {}", e),
            }))
        }
    }
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
// Greeks Calculation
// ============================================================================

/// Get Greeks for an option.
///
/// Calculates and returns the Greeks (Delta, Gamma, Theta, Vega, Rho) for a specific option.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/greeks",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'")
    ),
    responses(
        (status = 200, description = "Greeks for the option", body = GreeksResponse),
        (status = 404, description = "Not found")
    ),
    tag = "Greeks"
)]
pub async fn get_option_greeks(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
) -> Result<Json<GreeksResponse>, ApiError> {
    use optionstratlib::greeks::Greeks;
    use optionstratlib::model::option::Options;
    use optionstratlib::prelude::{OptionType, Positive};

    let option_style = parse_option_style(&style)?;

    // Verify the option exists
    let underlying_book = state
        .manager
        .get(&underlying)
        .map_err(|_| ApiError::UnderlyingNotFound(underlying.clone()))?;

    let expiration = find_expiration_by_str(&underlying_book, &exp_str)
        .ok_or_else(|| ApiError::ExpirationNotFound(exp_str.clone()))?;

    let _exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|_| ApiError::ExpirationNotFound(exp_str.clone()))?;

    // Build symbol
    let style_char = match option_style {
        OptionStyle::Call => "C",
        OptionStyle::Put => "P",
    };
    let symbol = format!("{}-{}-{}-{}", underlying, exp_str, strike, style_char);

    // Get spot price from price simulator (default to strike if not available)
    let spot_price = state
        .price_simulator
        .as_ref()
        .and_then(|sim| sim.get_price(&underlying))
        .unwrap_or(strike);

    // Default IV (30%) and risk-free rate (5%)
    let iv = 0.30;
    let risk_free_rate = 0.05;

    // Calculate time to expiry in years
    let now = chrono::Utc::now();
    let expiry_date = expiration
        .get_date()
        .map_err(|_| ApiError::InvalidRequest("Failed to parse expiration date".to_string()))?;
    let _days_to_expiry = (expiry_date - now).num_days().max(1) as f64;

    // Create Options struct for Greeks calculation
    let option_type = match option_style {
        OptionStyle::Call => OptionType::European,
        OptionStyle::Put => OptionType::European,
    };

    let side = match option_style {
        OptionStyle::Call => optionstratlib::prelude::Side::Long,
        OptionStyle::Put => optionstratlib::prelude::Side::Long,
    };

    // Create Positive values
    let spot_pos = Positive::new(spot_price as f64)
        .map_err(|_| ApiError::InvalidRequest("Invalid spot price".to_string()))?;
    let strike_pos = Positive::new(strike as f64)
        .map_err(|_| ApiError::InvalidRequest("Invalid strike price".to_string()))?;
    let iv_pos =
        Positive::new(iv).map_err(|_| ApiError::InvalidRequest("Invalid IV".to_string()))?;
    let quantity_pos =
        Positive::new(1.0).map_err(|_| ApiError::InvalidRequest("Invalid quantity".to_string()))?;
    let dividend_yield = Positive::new(0.0)
        .map_err(|_| ApiError::InvalidRequest("Invalid dividend yield".to_string()))?;

    use rust_decimal::Decimal;

    // Build the Options struct
    let option = Options::new(
        option_type,
        side,
        underlying.clone(),
        strike_pos,
        expiration,
        iv_pos,
        quantity_pos,
        spot_pos,
        Decimal::from_f64_retain(risk_free_rate).unwrap_or_default(),
        option_style,
        dividend_yield,
        None, // exotic_params
    );

    // Calculate Greeks
    let greeks_result = option.greeks();
    let theoretical_value = option.calculate_price_black_scholes().unwrap_or_default();

    let greeks_data = match greeks_result {
        Ok(greek) => {
            use rust_decimal::prelude::ToPrimitive;
            GreeksData {
                delta: greek.delta.to_f64().unwrap_or(0.0),
                gamma: greek.gamma.to_f64().unwrap_or(0.0),
                theta: greek.theta.to_f64().unwrap_or(0.0),
                vega: greek.vega.to_f64().unwrap_or(0.0),
                rho: greek.rho.to_f64().unwrap_or(0.0),
                vanna: Some(greek.vanna.to_f64().unwrap_or(0.0)),
                vomma: Some(greek.vomma.to_f64().unwrap_or(0.0)),
                charm: Some(greek.charm.to_f64().unwrap_or(0.0)),
                color: Some(greek.color.to_f64().unwrap_or(0.0)),
            }
        }
        Err(_) => GreeksData::default(),
    };

    let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;

    use rust_decimal::prelude::ToPrimitive;
    Ok(Json(GreeksResponse {
        symbol,
        greeks: greeks_data,
        iv,
        theoretical_value: theoretical_value.to_f64().unwrap_or(0.0),
        timestamp_ms,
    }))
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

// ============================================================================
// Last Trade Information
// ============================================================================

/// Get the last trade information for an option.
///
/// Returns the most recent trade that occurred for the specified option contract.
/// If no trades have occurred, returns a 404 Not Found error.
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
        (status = 404, description = "No trades found for this option")
    ),
    tag = "Options"
)]
pub async fn get_last_trade(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
) -> Result<Json<LastTradeResponse>, ApiError> {
    let option_style = parse_option_style(&style)?;

    // Build the symbol key to look up in the last trades map
    let style_char = match option_style {
        OptionStyle::Call => "C",
        OptionStyle::Put => "P",
    };
    let symbol = format!("{}-{}-{}-{}", underlying, exp_str, strike, style_char);

    // Look up the last trade for this symbol
    match state.last_trades.get(&symbol) {
        Some(trade_info) => Ok(Json(LastTradeResponse::from(trade_info.clone()))),
        None => Err(ApiError::NotFound(format!(
            "No trades found for option {}",
            symbol
        ))),
    }
}

// ============================================================================
// OHLC Historical Data
// ============================================================================

/// Get OHLC (candlestick) historical data for an option.
///
/// Returns OHLC bars aggregated from trades at the specified interval.
/// Supports filtering by time range and limiting the number of bars returned.
#[utoipa::path(
    get,
    path = "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/ohlc",
    params(
        ("underlying" = String, Path, description = "Underlying symbol"),
        ("expiration" = String, Path, description = "Expiration date"),
        ("strike" = u64, Path, description = "Strike price"),
        ("style" = String, Path, description = "Option style: 'call' or 'put'"),
        ("interval" = String, Query, description = "Bar interval: 1m, 5m, 15m, 1h, 4h, 1d"),
        ("from" = Option<u64>, Query, description = "Start timestamp in seconds (optional)"),
        ("to" = Option<u64>, Query, description = "End timestamp in seconds (optional)"),
        ("limit" = Option<usize>, Query, description = "Maximum number of bars (default 500)")
    ),
    responses(
        (status = 200, description = "OHLC historical data", body = OhlcResponse),
        (status = 400, description = "Invalid interval"),
        (status = 404, description = "Not found")
    ),
    tag = "Options"
)]
pub async fn get_ohlc(
    State(state): State<Arc<AppState>>,
    Path((underlying, exp_str, strike, style)): Path<(String, String, u64, String)>,
    Query(query): Query<OhlcQuery>,
) -> Result<Json<OhlcResponse>, ApiError> {
    let option_style = parse_option_style(&style)?;

    // Parse interval
    let interval: OhlcInterval = query
        .interval
        .parse()
        .map_err(|e: String| ApiError::InvalidRequest(e))?;

    // Build the symbol key
    let style_char = match option_style {
        OptionStyle::Call => "C",
        OptionStyle::Put => "P",
    };
    let symbol = format!("{}-{}-{}-{}", underlying, exp_str, strike, style_char);

    // Get bars from aggregator
    let limit = query.limit.unwrap_or(500).min(1000); // Cap at 1000
    let bars = state
        .ohlc_aggregator
        .get_bars(&symbol, interval, query.from, query.to, limit);

    Ok(Json(OhlcResponse {
        symbol,
        interval: interval.to_string(),
        bars,
    }))
}

// ============================================================================
// Order Status and Query
// ============================================================================

/// Get the status of a single order by its ID.
///
/// Returns detailed information about the order including its current status,
/// fill history, and timestamps.
#[utoipa::path(
    get,
    path = "/api/v1/orders/{order_id}",
    params(
        ("order_id" = String, Path, description = "Order identifier")
    ),
    responses(
        (status = 200, description = "Order status", body = OrderStatusResponse),
        (status = 404, description = "Order not found")
    ),
    tag = "Orders"
)]
pub async fn get_order_status(
    State(state): State<Arc<AppState>>,
    Path(order_id): Path<String>,
) -> Result<Json<OrderStatusResponse>, ApiError> {
    match state.orders.get(&order_id) {
        Some(order_info) => Ok(Json(OrderStatusResponse::from(order_info.clone()))),
        None => Err(ApiError::NotFound(format!("Order not found: {}", order_id))),
    }
}

/// List orders with optional filters and pagination.
///
/// Supports filtering by underlying symbol, order status, and side.
/// Results are paginated with configurable limit and offset.
#[utoipa::path(
    get,
    path = "/api/v1/orders",
    params(
        ("underlying" = Option<String>, Query, description = "Filter by underlying symbol"),
        ("status" = Option<String>, Query, description = "Filter by order status"),
        ("side" = Option<String>, Query, description = "Filter by order side"),
        ("limit" = Option<usize>, Query, description = "Pagination limit (default: 100)"),
        ("offset" = Option<usize>, Query, description = "Pagination offset (default: 0)")
    ),
    responses(
        (status = 200, description = "List of orders", body = OrderListResponse),
        (status = 400, description = "Invalid query parameters")
    ),
    tag = "Orders"
)]
pub async fn list_orders(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OrderListQuery>,
) -> Result<Json<OrderListResponse>, ApiError> {
    // Parse status filter if provided
    let status_filter: Option<OrderStatus> = if let Some(ref status_str) = query.status {
        Some(
            status_str
                .parse()
                .map_err(|e: String| ApiError::InvalidRequest(e))?,
        )
    } else {
        None
    };

    // Parse side filter if provided
    let side_filter: Option<OrderSide> = if let Some(ref side_str) = query.side {
        match side_str.to_lowercase().as_str() {
            "buy" => Some(OrderSide::Buy),
            "sell" => Some(OrderSide::Sell),
            _ => {
                return Err(ApiError::InvalidRequest(format!(
                    "Invalid side: {}. Use 'buy' or 'sell'",
                    side_str
                )));
            }
        }
    } else {
        None
    };

    // Collect and filter orders
    let mut filtered_orders: Vec<OrderInfo> = state
        .orders
        .iter()
        .filter(|entry| {
            let order = entry.value();

            // Filter by underlying
            if let Some(ref underlying) = query.underlying
                && &order.underlying != underlying
            {
                return false;
            }

            // Filter by status
            if let Some(status) = status_filter
                && order.status != status
            {
                return false;
            }

            // Filter by side
            if let Some(side) = side_filter
                && order.side != side
            {
                return false;
            }

            true
        })
        .map(|entry| entry.value().clone())
        .collect();

    // Sort by creation time (newest first)
    filtered_orders.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));

    let total = filtered_orders.len();

    // Apply pagination
    let paginated: Vec<OrderStatusResponse> = filtered_orders
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(OrderStatusResponse::from)
        .collect();

    Ok(Json(OrderListResponse {
        orders: paginated,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

// ============================================================================
// Bulk Order Operations
// ============================================================================

/// Helper function to submit a single order from a bulk request.
fn submit_single_order(
    state: &Arc<AppState>,
    item: &BulkOrderItem,
) -> Result<(OrderId, String), String> {
    // Parse option style
    let option_style = match item.style.to_lowercase().as_str() {
        "call" => OptionStyle::Call,
        "put" => OptionStyle::Put,
        _ => return Err(format!("Invalid option style: {}", item.style)),
    };

    // Parse side
    let side = match item.side.to_lowercase().as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => return Err(format!("Invalid side: {}", item.side)),
    };

    // Get or create underlying
    let underlying_book = state.manager.get_or_create(&item.underlying);

    // Find expiration
    let expiration = find_expiration_by_str(&underlying_book, &item.expiration)
        .ok_or_else(|| format!("Expiration not found: {}", item.expiration))?;

    // Get expiration book
    let exp_book = underlying_book
        .get_expiration(&expiration)
        .map_err(|e| format!("Failed to get expiration: {}", e))?;

    // Get strike book
    let strike_book = exp_book
        .get_strike(item.strike)
        .map_err(|e| format!("Strike not found: {}", e))?;

    // Get option book
    let option_book = strike_book.get(option_style);

    // Generate order ID and submit
    let order_id = OrderId::new();
    option_book
        .add_limit_order(order_id, side, item.price, item.quantity)
        .map_err(|e| format!("Failed to add order: {}", e))?;

    // Build symbol for tracking
    let style_char = match option_style {
        OptionStyle::Call => "C",
        OptionStyle::Put => "P",
    };
    let symbol = format!(
        "{}-{}-{}-{}",
        item.underlying, item.expiration, item.strike, style_char
    );

    // Track order in AppState
    let order_side = match side {
        Side::Buy => OrderSide::Buy,
        Side::Sell => OrderSide::Sell,
    };
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let order_info = OrderInfo {
        order_id: order_id.to_string(),
        symbol: symbol.clone(),
        underlying: item.underlying.clone(),
        expiration: item.expiration.clone(),
        strike: item.strike,
        style: item.style.clone(),
        side: order_side,
        price: item.price,
        original_quantity: item.quantity,
        remaining_quantity: item.quantity,
        filled_quantity: 0,
        status: OrderStatus::Active,
        time_in_force: OrderTimeInForce::Gtc,
        created_at_ms: now,
        updated_at_ms: now,
        fills: Vec::new(),
    };
    state.orders.insert(order_id.to_string(), order_info);

    Ok((order_id, symbol))
}

/// Submit multiple orders in a single request.
///
/// Supports atomic mode where all orders must succeed or none are submitted.
#[utoipa::path(
    post,
    path = "/api/v1/orders/bulk",
    request_body = BulkOrderRequest,
    responses(
        (status = 200, description = "Bulk order submission results", body = BulkOrderResponse),
        (status = 400, description = "Invalid request")
    ),
    tag = "Orders"
)]
pub async fn bulk_submit_orders(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BulkOrderRequest>,
) -> Result<Json<BulkOrderResponse>, ApiError> {
    if body.orders.is_empty() {
        return Err(ApiError::InvalidRequest(
            "Orders array cannot be empty".to_string(),
        ));
    }

    let mut results: Vec<BulkOrderResultItem> = Vec::with_capacity(body.orders.len());
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut submitted_order_ids: Vec<String> = Vec::new();

    for (index, item) in body.orders.iter().enumerate() {
        match submit_single_order(&state, item) {
            Ok((order_id, _symbol)) => {
                let order_id_str = order_id.to_string();
                submitted_order_ids.push(order_id_str.clone());
                results.push(BulkOrderResultItem {
                    index,
                    order_id: Some(order_id_str),
                    status: BulkOrderStatus::Accepted,
                    error: None,
                });
                success_count += 1;
            }
            Err(error) => {
                results.push(BulkOrderResultItem {
                    index,
                    order_id: None,
                    status: BulkOrderStatus::Rejected,
                    error: Some(error.clone()),
                });
                failure_count += 1;

                // If atomic mode and we have a failure, rollback all submitted orders
                if body.atomic {
                    // Cancel all previously submitted orders
                    for order_id_str in &submitted_order_ids {
                        // Remove from tracking
                        state.orders.remove(order_id_str);
                        // Note: We can't easily cancel from the order book without
                        // knowing the full path, so we just remove from tracking
                    }

                    return Ok(Json(BulkOrderResponse {
                        success_count: 0,
                        failure_count: body.orders.len(),
                        results: body
                            .orders
                            .iter()
                            .enumerate()
                            .map(|(i, _)| {
                                if i == index {
                                    BulkOrderResultItem {
                                        index: i,
                                        order_id: None,
                                        status: BulkOrderStatus::Rejected,
                                        error: Some(error.clone()),
                                    }
                                } else if i < index {
                                    BulkOrderResultItem {
                                        index: i,
                                        order_id: None,
                                        status: BulkOrderStatus::Rejected,
                                        error: Some(
                                            "Rolled back due to atomic failure".to_string(),
                                        ),
                                    }
                                } else {
                                    BulkOrderResultItem {
                                        index: i,
                                        order_id: None,
                                        status: BulkOrderStatus::Rejected,
                                        error: Some(
                                            "Not attempted due to atomic failure".to_string(),
                                        ),
                                    }
                                }
                            })
                            .collect(),
                    }));
                }
            }
        }
    }

    Ok(Json(BulkOrderResponse {
        success_count,
        failure_count,
        results,
    }))
}

/// Cancel multiple orders by their IDs.
#[utoipa::path(
    delete,
    path = "/api/v1/orders/bulk",
    request_body = BulkCancelRequest,
    responses(
        (status = 200, description = "Bulk cancellation results", body = BulkCancelResponse),
        (status = 400, description = "Invalid request")
    ),
    tag = "Orders"
)]
pub async fn bulk_cancel_orders(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BulkCancelRequest>,
) -> Result<Json<BulkCancelResponse>, ApiError> {
    if body.order_ids.is_empty() {
        return Err(ApiError::InvalidRequest(
            "Order IDs array cannot be empty".to_string(),
        ));
    }

    let mut results: Vec<BulkCancelResultItem> = Vec::with_capacity(body.order_ids.len());
    let mut success_count = 0;
    let mut failure_count = 0;

    for order_id_str in &body.order_ids {
        // Try to get order info to find the order book location
        if let Some(order_info) = state.orders.get(order_id_str) {
            // Parse order ID
            if let Ok(order_id) = order_id_str.parse::<OrderId>() {
                // Try to find and cancel the order
                let option_style = match order_info.style.to_lowercase().as_str() {
                    "call" => OptionStyle::Call,
                    "put" => OptionStyle::Put,
                    _ => {
                        results.push(BulkCancelResultItem {
                            order_id: order_id_str.clone(),
                            canceled: false,
                            error: Some("Invalid option style in order info".to_string()),
                        });
                        failure_count += 1;
                        continue;
                    }
                };

                // Try to get the order book and cancel
                if let Ok(underlying_book) = state.manager.get(&order_info.underlying)
                    && let Some(expiration) =
                        find_expiration_by_str(&underlying_book, &order_info.expiration)
                    && let Ok(exp_book) = underlying_book.get_expiration(&expiration)
                    && let Ok(strike_book) = exp_book.get_strike(order_info.strike)
                {
                    let option_book = strike_book.get(option_style);
                    match option_book.cancel_order(order_id) {
                        Ok(true) => {
                            // Remove from tracking
                            drop(order_info);
                            state.orders.remove(order_id_str);
                            results.push(BulkCancelResultItem {
                                order_id: order_id_str.clone(),
                                canceled: true,
                                error: None,
                            });
                            success_count += 1;
                            continue;
                        }
                        Ok(false) => {
                            results.push(BulkCancelResultItem {
                                order_id: order_id_str.clone(),
                                canceled: false,
                                error: Some("Order not found in book".to_string()),
                            });
                            failure_count += 1;
                            continue;
                        }
                        Err(e) => {
                            results.push(BulkCancelResultItem {
                                order_id: order_id_str.clone(),
                                canceled: false,
                                error: Some(format!("Cancel failed: {}", e)),
                            });
                            failure_count += 1;
                            continue;
                        }
                    }
                }

                // If we get here, something in the path lookup failed
                results.push(BulkCancelResultItem {
                    order_id: order_id_str.clone(),
                    canceled: false,
                    error: Some("Order book path not found".to_string()),
                });
                failure_count += 1;
            } else {
                results.push(BulkCancelResultItem {
                    order_id: order_id_str.clone(),
                    canceled: false,
                    error: Some("Invalid order ID format".to_string()),
                });
                failure_count += 1;
            }
        } else {
            results.push(BulkCancelResultItem {
                order_id: order_id_str.clone(),
                canceled: false,
                error: Some("Order not found".to_string()),
            });
            failure_count += 1;
        }
    }

    Ok(Json(BulkCancelResponse {
        success_count,
        failure_count,
        results,
    }))
}

/// Cancel all orders matching the specified filters.
#[utoipa::path(
    delete,
    path = "/api/v1/orders/cancel-all",
    params(
        ("underlying" = Option<String>, Query, description = "Filter by underlying symbol"),
        ("expiration" = Option<String>, Query, description = "Filter by expiration date"),
        ("side" = Option<String>, Query, description = "Filter by order side"),
        ("style" = Option<String>, Query, description = "Filter by option style")
    ),
    responses(
        (status = 200, description = "Cancel all results", body = CancelAllResponse),
        (status = 400, description = "Invalid query parameters")
    ),
    tag = "Orders"
)]
pub async fn cancel_all_orders(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CancelAllQuery>,
) -> Result<Json<CancelAllResponse>, ApiError> {
    // Parse side filter if provided
    let side_filter: Option<OrderSide> = if let Some(ref side_str) = query.side {
        match side_str.to_lowercase().as_str() {
            "buy" => Some(OrderSide::Buy),
            "sell" => Some(OrderSide::Sell),
            _ => {
                return Err(ApiError::InvalidRequest(format!(
                    "Invalid side: {}. Use 'buy' or 'sell'",
                    side_str
                )));
            }
        }
    } else {
        None
    };

    // Parse style filter if provided
    let style_filter: Option<OptionStyle> = if let Some(ref style_str) = query.style {
        match style_str.to_lowercase().as_str() {
            "call" => Some(OptionStyle::Call),
            "put" => Some(OptionStyle::Put),
            _ => {
                return Err(ApiError::InvalidRequest(format!(
                    "Invalid style: {}. Use 'call' or 'put'",
                    style_str
                )));
            }
        }
    } else {
        None
    };

    let mut canceled_count = 0;
    let mut failed_count = 0;

    // Collect order IDs to cancel (to avoid holding locks while canceling)
    let orders_to_cancel: Vec<(String, OrderInfo)> = state
        .orders
        .iter()
        .filter(|entry| {
            let order = entry.value();

            // Filter by underlying
            if let Some(ref underlying) = query.underlying
                && &order.underlying != underlying
            {
                return false;
            }

            // Filter by expiration
            if let Some(ref expiration) = query.expiration
                && &order.expiration != expiration
            {
                return false;
            }

            // Filter by side
            if let Some(side) = side_filter
                && order.side != side
            {
                return false;
            }

            // Filter by style
            if let Some(style) = style_filter {
                let order_style = match order.style.to_lowercase().as_str() {
                    "call" => OptionStyle::Call,
                    "put" => OptionStyle::Put,
                    _ => return false,
                };
                if order_style != style {
                    return false;
                }
            }

            // Only cancel open orders
            order.status == OrderStatus::Active
        })
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect();

    // Cancel each matching order
    for (order_id_str, order_info) in orders_to_cancel {
        if let Ok(order_id) = order_id_str.parse::<OrderId>() {
            let option_style = match order_info.style.to_lowercase().as_str() {
                "call" => OptionStyle::Call,
                "put" => OptionStyle::Put,
                _ => {
                    failed_count += 1;
                    continue;
                }
            };

            if let Ok(underlying_book) = state.manager.get(&order_info.underlying)
                && let Some(expiration) =
                    find_expiration_by_str(&underlying_book, &order_info.expiration)
                && let Ok(exp_book) = underlying_book.get_expiration(&expiration)
                && let Ok(strike_book) = exp_book.get_strike(order_info.strike)
            {
                let option_book = strike_book.get(option_style);
                match option_book.cancel_order(order_id) {
                    Ok(true) => {
                        state.orders.remove(&order_id_str);
                        canceled_count += 1;
                        continue;
                    }
                    Ok(false) | Err(_) => {
                        failed_count += 1;
                        continue;
                    }
                }
            }

            failed_count += 1;
        } else {
            failed_count += 1;
        }
    }

    Ok(Json(CancelAllResponse {
        canceled_count,
        failed_count,
    }))
}

// ============================================================================
// Position and Inventory Tracking
// ============================================================================

/// Get a specific position by symbol.
///
/// Returns detailed information about the position including P&L calculations.
#[utoipa::path(
    get,
    path = "/api/v1/positions/{symbol}",
    params(
        ("symbol" = String, Path, description = "Option symbol (e.g., AAPL-20240329-150-C)")
    ),
    responses(
        (status = 200, description = "Position details", body = PositionResponse),
        (status = 404, description = "Position not found")
    ),
    tag = "Positions"
)]
pub async fn get_position(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
) -> Result<Json<PositionResponse>, ApiError> {
    match state.positions.get(&symbol) {
        Some(position) => {
            // Get current market price from quote
            let current_price = get_current_price_for_symbol(&state, &symbol).unwrap_or(0);
            let unrealized_pnl = position.unrealized_pnl(current_price);
            let notional_value = position.notional_value(current_price);

            // TODO: Calculate delta from option pricer when available
            let delta_exposure = 0.0;

            Ok(Json(PositionResponse {
                symbol: position.symbol.clone(),
                underlying: position.underlying.clone(),
                quantity: position.quantity,
                average_price: position.average_price,
                current_price,
                unrealized_pnl,
                realized_pnl: position.realized_pnl,
                delta_exposure,
                notional_value,
            }))
        }
        None => Err(ApiError::NotFound(format!(
            "Position not found: {}",
            symbol
        ))),
    }
}

/// List all positions with optional filtering.
///
/// Returns all positions with aggregate summary statistics.
#[utoipa::path(
    get,
    path = "/api/v1/positions",
    params(
        ("underlying" = Option<String>, Query, description = "Filter by underlying symbol")
    ),
    responses(
        (status = 200, description = "List of positions", body = PositionsListResponse)
    ),
    tag = "Positions"
)]
pub async fn list_positions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PositionQuery>,
) -> Result<Json<PositionsListResponse>, ApiError> {
    let mut total_unrealized_pnl = 0i64;
    let mut total_realized_pnl = 0i64;
    let mut net_delta = 0.0f64;

    let positions: Vec<PositionResponse> = state
        .positions
        .iter()
        .filter(|entry| {
            if let Some(ref underlying) = query.underlying {
                &entry.value().underlying == underlying
            } else {
                true
            }
        })
        .filter(|entry| entry.value().quantity != 0) // Only include open positions
        .map(|entry| {
            let position = entry.value();
            let current_price = get_current_price_for_symbol(&state, &position.symbol).unwrap_or(0);
            let unrealized_pnl = position.unrealized_pnl(current_price);
            let notional_value = position.notional_value(current_price);

            // TODO: Calculate delta from option pricer when available
            let delta_exposure = 0.0;

            total_unrealized_pnl += unrealized_pnl;
            total_realized_pnl += position.realized_pnl;
            net_delta += delta_exposure;

            PositionResponse {
                symbol: position.symbol.clone(),
                underlying: position.underlying.clone(),
                quantity: position.quantity,
                average_price: position.average_price,
                current_price,
                unrealized_pnl,
                realized_pnl: position.realized_pnl,
                delta_exposure,
                notional_value,
            }
        })
        .collect();

    let position_count = positions.len();

    Ok(Json(PositionsListResponse {
        positions,
        summary: PositionSummary {
            total_unrealized_pnl,
            total_realized_pnl,
            net_delta,
            position_count,
        },
    }))
}

/// Helper function to get current market price for a symbol.
fn get_current_price_for_symbol(state: &AppState, symbol: &str) -> Option<u128> {
    // Parse symbol to get underlying, expiration, strike, style
    let parts: Vec<&str> = symbol.split('-').collect();
    if parts.len() != 4 {
        return None;
    }

    let underlying = parts[0];
    let exp_str = parts[1];
    let strike: u64 = parts[2].parse().ok()?;
    let style_str = parts[3];

    let style = match style_str.to_uppercase().as_str() {
        "C" | "CALL" => OptionStyle::Call,
        "P" | "PUT" => OptionStyle::Put,
        _ => return None,
    };

    let expiration = parse_expiration(exp_str).ok()?;

    // Get the order book and quote
    let underlying_book = state.manager.get(underlying).ok()?;
    let exp_book = underlying_book.get_expiration(&expiration).ok()?;
    let strike_book = exp_book.get_strike(strike).ok()?;
    let option_book = strike_book.get(style);
    let quote = option_book.best_quote();

    // Use mid price if available, otherwise best bid or ask
    match (quote.bid_price(), quote.ask_price()) {
        (Some(bid), Some(ask)) => Some((bid + ask) / 2),
        (Some(bid), None) => Some(bid),
        (None, Some(ask)) => Some(ask),
        (None, None) => None,
    }
}

/// Updates position based on a fill.
///
/// This function should be called after each fill to update position tracking.
pub fn update_position_on_fill(
    state: &AppState,
    symbol: &str,
    underlying: &str,
    side: OrderSide,
    quantity: u64,
    price: u128,
    timestamp_ms: u64,
) {
    let fill_quantity = match side {
        OrderSide::Buy => quantity as i64,
        OrderSide::Sell => -(quantity as i64),
    };

    state
        .positions
        .entry(symbol.to_string())
        .and_modify(|pos| {
            pos.update(fill_quantity, price, timestamp_ms);
        })
        .or_insert_with(|| {
            PositionInfo::new(
                symbol.to_string(),
                underlying.to_string(),
                fill_quantity,
                price,
                timestamp_ms,
            )
        });
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

    // ========================================================================
    // Last Trade Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_last_trade_not_found() {
        let state = create_test_state();

        // Create underlying, expiration, strike (but no trades)
        let underlying_book = state.manager.get_or_create("TRADE1");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        drop(exp_book.get_or_create_strike(100));

        // Try to get last trade - should return 404
        let result = get_last_trade(
            State(state.clone()),
            Path((
                "TRADE1".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::NotFound(msg) => {
                assert!(msg.contains("No trades found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_get_last_trade_success() {
        use crate::models::{LastTradeInfo, OrderSide};

        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("TRADE2");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        drop(exp_book.get_or_create_strike(100));

        // Manually insert a last trade into the state
        let symbol = "TRADE2-20251231-100-C".to_string();
        let trade_info = LastTradeInfo {
            symbol: symbol.clone(),
            price: 150,
            quantity: 50,
            side: OrderSide::Buy,
            timestamp_ms: 1234567890,
            trade_id: "test-trade-id".to_string(),
        };
        state.last_trades.insert(symbol, trade_info);

        // Get last trade - should succeed
        let result = get_last_trade(
            State(state.clone()),
            Path((
                "TRADE2".to_string(),
                "20251231".to_string(),
                100u64,
                "call".to_string(),
            )),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.symbol, "TRADE2-20251231-100-C");
        assert_eq!(response.price, 150);
        assert_eq!(response.quantity, 50);
        assert_eq!(response.side, OrderSide::Buy);
        assert_eq!(response.timestamp_ms, 1234567890);
        assert_eq!(response.trade_id, "test-trade-id");
    }

    #[tokio::test]
    async fn test_get_last_trade_put_option() {
        use crate::models::{LastTradeInfo, OrderSide};

        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying_book = state.manager.get_or_create("TRADE3");
        let expiration = parse_expiration("20251231").unwrap();
        let exp_book = underlying_book.get_or_create_expiration(expiration);
        drop(exp_book.get_or_create_strike(100));

        // Manually insert a last trade for a put option
        let symbol = "TRADE3-20251231-100-P".to_string();
        let trade_info = LastTradeInfo {
            symbol: symbol.clone(),
            price: 200,
            quantity: 75,
            side: OrderSide::Sell,
            timestamp_ms: 9876543210,
            trade_id: "put-trade-id".to_string(),
        };
        state.last_trades.insert(symbol, trade_info);

        // Get last trade for put option - should succeed
        let result = get_last_trade(
            State(state.clone()),
            Path((
                "TRADE3".to_string(),
                "20251231".to_string(),
                100u64,
                "put".to_string(),
            )),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.symbol, "TRADE3-20251231-100-P");
        assert_eq!(response.price, 200);
        assert_eq!(response.quantity, 75);
        assert_eq!(response.side, OrderSide::Sell);
    }

    #[tokio::test]
    async fn test_get_last_trade_invalid_style() {
        let state = create_test_state();

        // Try to get last trade with invalid style
        let result = get_last_trade(
            State(state.clone()),
            Path((
                "TEST".to_string(),
                "20251231".to_string(),
                100u64,
                "invalid".to_string(),
            )),
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

    // ========================================================================
    // Order Status Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_order_status_not_found() {
        let state = create_test_state();

        let result = get_order_status(
            State(state.clone()),
            Path("nonexistent-order-id".to_string()),
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::NotFound(msg) => {
                assert!(msg.contains("Order not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_get_order_status_success() {
        use crate::models::{OrderInfo, OrderStatus, OrderTimeInForce};

        let state = create_test_state();

        // Insert an order into the state
        let order_id = "test-order-123".to_string();
        let order_info = OrderInfo {
            order_id: order_id.clone(),
            symbol: "AAPL-20251231-150-C".to_string(),
            underlying: "AAPL".to_string(),
            expiration: "20251231".to_string(),
            strike: 150,
            style: "call".to_string(),
            side: OrderSide::Buy,
            price: 100,
            original_quantity: 100,
            remaining_quantity: 60,
            filled_quantity: 40,
            status: OrderStatus::Partial,
            time_in_force: OrderTimeInForce::Gtc,
            created_at_ms: 1704067200000,
            updated_at_ms: 1704067500000,
            fills: vec![],
        };
        state.orders.insert(order_id.clone(), order_info);

        let result = get_order_status(State(state.clone()), Path(order_id.clone())).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.order_id, "test-order-123");
        assert_eq!(response.symbol, "AAPL-20251231-150-C");
        assert_eq!(response.side, OrderSide::Buy);
        assert_eq!(response.original_quantity, 100);
        assert_eq!(response.remaining_quantity, 60);
        assert_eq!(response.filled_quantity, 40);
        assert_eq!(response.status, OrderStatus::Partial);
    }

    #[tokio::test]
    async fn test_list_orders_empty() {
        let state = create_test_state();

        let result = list_orders(
            State(state.clone()),
            Query(OrderListQuery {
                underlying: None,
                status: None,
                side: None,
                limit: 100,
                offset: 0,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.orders.len(), 0);
        assert_eq!(response.total, 0);
    }

    #[tokio::test]
    async fn test_list_orders_with_filters() {
        use crate::models::{OrderInfo, OrderStatus, OrderTimeInForce};

        let state = create_test_state();

        // Insert multiple orders
        for i in 0..5 {
            let order_info = OrderInfo {
                order_id: format!("order-{}", i),
                symbol: format!("AAPL-20251231-{}-C", 150 + i * 5),
                underlying: if i < 3 {
                    "AAPL".to_string()
                } else {
                    "GOOG".to_string()
                },
                expiration: "20251231".to_string(),
                strike: 150 + i * 5,
                style: "call".to_string(),
                side: if i % 2 == 0 {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                },
                price: 100,
                original_quantity: 100,
                remaining_quantity: 100,
                filled_quantity: 0,
                status: OrderStatus::Active,
                time_in_force: OrderTimeInForce::Gtc,
                created_at_ms: 1704067200000 + i * 1000,
                updated_at_ms: 1704067200000 + i * 1000,
                fills: vec![],
            };
            state.orders.insert(format!("order-{}", i), order_info);
        }

        // Filter by underlying
        let result = list_orders(
            State(state.clone()),
            Query(OrderListQuery {
                underlying: Some("AAPL".to_string()),
                status: None,
                side: None,
                limit: 100,
                offset: 0,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.total, 3);

        // Filter by side
        let result = list_orders(
            State(state.clone()),
            Query(OrderListQuery {
                underlying: None,
                status: None,
                side: Some("buy".to_string()),
                limit: 100,
                offset: 0,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.total, 3); // orders 0, 2, 4 are buy
    }

    #[tokio::test]
    async fn test_list_orders_pagination() {
        use crate::models::{OrderInfo, OrderStatus, OrderTimeInForce};

        let state = create_test_state();

        // Insert 10 orders
        for i in 0..10 {
            let order_info = OrderInfo {
                order_id: format!("order-{}", i),
                symbol: format!("AAPL-20251231-{}-C", 150 + i * 5),
                underlying: "AAPL".to_string(),
                expiration: "20251231".to_string(),
                strike: 150 + i * 5,
                style: "call".to_string(),
                side: OrderSide::Buy,
                price: 100,
                original_quantity: 100,
                remaining_quantity: 100,
                filled_quantity: 0,
                status: OrderStatus::Active,
                time_in_force: OrderTimeInForce::Gtc,
                created_at_ms: 1704067200000 + i * 1000,
                updated_at_ms: 1704067200000 + i * 1000,
                fills: vec![],
            };
            state.orders.insert(format!("order-{}", i), order_info);
        }

        // Get first page (limit 3)
        let result = list_orders(
            State(state.clone()),
            Query(OrderListQuery {
                underlying: None,
                status: None,
                side: None,
                limit: 3,
                offset: 0,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.orders.len(), 3);
        assert_eq!(response.total, 10);
        assert_eq!(response.limit, 3);
        assert_eq!(response.offset, 0);

        // Get second page
        let result = list_orders(
            State(state.clone()),
            Query(OrderListQuery {
                underlying: None,
                status: None,
                side: None,
                limit: 3,
                offset: 3,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.orders.len(), 3);
        assert_eq!(response.offset, 3);
    }

    #[tokio::test]
    async fn test_list_orders_invalid_side() {
        let state = create_test_state();

        let result = list_orders(
            State(state.clone()),
            Query(OrderListQuery {
                underlying: None,
                status: None,
                side: Some("invalid".to_string()),
                limit: 100,
                offset: 0,
            }),
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::InvalidRequest(msg) => {
                assert!(msg.contains("Invalid side"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    // ========================================================================
    // Position Tracking Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_position_not_found() {
        let state = create_test_state();

        let result = get_position(
            State(state.clone()),
            Path("AAPL-20251231-150-C".to_string()),
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::NotFound(msg) => {
                assert!(msg.contains("Position not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_get_position_success() {
        let state = create_test_state();

        // Insert a position
        let symbol = "AAPL-20251231-150-C".to_string();
        let position =
            PositionInfo::new(symbol.clone(), "AAPL".to_string(), 100, 500, 1704067200000);
        state.positions.insert(symbol.clone(), position);

        let result = get_position(State(state.clone()), Path(symbol)).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.symbol, "AAPL-20251231-150-C");
        assert_eq!(response.underlying, "AAPL");
        assert_eq!(response.quantity, 100);
        assert_eq!(response.average_price, 500);
    }

    #[tokio::test]
    async fn test_list_positions_empty() {
        let state = create_test_state();

        let result = list_positions(
            State(state.clone()),
            Query(PositionQuery { underlying: None }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.positions.len(), 0);
        assert_eq!(response.summary.position_count, 0);
    }

    #[tokio::test]
    async fn test_list_positions_with_filter() {
        let state = create_test_state();

        // Insert positions for different underlyings
        state.positions.insert(
            "AAPL-20251231-150-C".to_string(),
            PositionInfo::new(
                "AAPL-20251231-150-C".to_string(),
                "AAPL".to_string(),
                100,
                500,
                1704067200000,
            ),
        );
        state.positions.insert(
            "GOOG-20251231-100-C".to_string(),
            PositionInfo::new(
                "GOOG-20251231-100-C".to_string(),
                "GOOG".to_string(),
                50,
                1000,
                1704067200000,
            ),
        );

        // Filter by AAPL
        let result = list_positions(
            State(state.clone()),
            Query(PositionQuery {
                underlying: Some("AAPL".to_string()),
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.positions.len(), 1);
        assert_eq!(response.positions[0].underlying, "AAPL");
    }

    #[tokio::test]
    async fn test_position_update_on_buy() {
        let state = create_test_state();

        // Update position with a buy
        update_position_on_fill(
            &state,
            "AAPL-20251231-150-C",
            "AAPL",
            OrderSide::Buy,
            100,
            500,
            1704067200000,
        );

        let position = state.positions.get("AAPL-20251231-150-C").unwrap();
        assert_eq!(position.quantity, 100);
        assert_eq!(position.average_price, 500);
    }

    #[tokio::test]
    async fn test_position_update_on_sell() {
        let state = create_test_state();

        // First buy
        update_position_on_fill(
            &state,
            "AAPL-20251231-150-C",
            "AAPL",
            OrderSide::Buy,
            100,
            500,
            1704067200000,
        );

        // Then sell (close position)
        update_position_on_fill(
            &state,
            "AAPL-20251231-150-C",
            "AAPL",
            OrderSide::Sell,
            50,
            600,
            1704067300000,
        );

        let position = state.positions.get("AAPL-20251231-150-C").unwrap();
        assert_eq!(position.quantity, 50); // 100 - 50
        assert_eq!(position.realized_pnl, 5000); // (600 - 500) * 50
    }

    #[tokio::test]
    async fn test_position_pnl_calculation() {
        let position = PositionInfo::new(
            "AAPL-20251231-150-C".to_string(),
            "AAPL".to_string(),
            100,
            500,
            1704067200000,
        );

        // Current price is 600, so unrealized P&L = (600 - 500) * 100 = 10000
        let unrealized = position.unrealized_pnl(600);
        assert_eq!(unrealized, 10000);

        // Notional value = 600 * 100 = 60000
        let notional = position.notional_value(600);
        assert_eq!(notional, 60000);
    }

    #[tokio::test]
    async fn test_position_short_pnl() {
        let position = PositionInfo::new(
            "AAPL-20251231-150-C".to_string(),
            "AAPL".to_string(),
            -100, // Short position
            500,
            1704067200000,
        );

        // Current price is 400, so unrealized P&L = (400 - 500) * -100 = 10000 (profit)
        let unrealized = position.unrealized_pnl(400);
        assert_eq!(unrealized, 10000);

        // Current price is 600, so unrealized P&L = (600 - 500) * -100 = -10000 (loss)
        let unrealized_loss = position.unrealized_pnl(600);
        assert_eq!(unrealized_loss, -10000);
    }

    #[test]
    fn test_api_time_in_force_serialization() {
        use crate::models::ApiTimeInForce;

        let gtc = ApiTimeInForce::Gtc;
        let json = serde_json::to_string(&gtc).unwrap();
        assert_eq!(json, "\"GTC\"");

        let ioc = ApiTimeInForce::Ioc;
        let json = serde_json::to_string(&ioc).unwrap();
        assert_eq!(json, "\"IOC\"");

        let fok = ApiTimeInForce::Fok;
        let json = serde_json::to_string(&fok).unwrap();
        assert_eq!(json, "\"FOK\"");

        let gtd = ApiTimeInForce::Gtd;
        let json = serde_json::to_string(&gtd).unwrap();
        assert_eq!(json, "\"GTD\"");
    }

    #[test]
    fn test_api_time_in_force_deserialization() {
        use crate::models::ApiTimeInForce;

        let gtc: ApiTimeInForce = serde_json::from_str("\"GTC\"").unwrap();
        assert_eq!(gtc, ApiTimeInForce::Gtc);

        let ioc: ApiTimeInForce = serde_json::from_str("\"IOC\"").unwrap();
        assert_eq!(ioc, ApiTimeInForce::Ioc);

        let fok: ApiTimeInForce = serde_json::from_str("\"FOK\"").unwrap();
        assert_eq!(fok, ApiTimeInForce::Fok);

        let gtd: ApiTimeInForce = serde_json::from_str("\"GTD\"").unwrap();
        assert_eq!(gtd, ApiTimeInForce::Gtd);
    }

    #[test]
    fn test_api_time_in_force_default() {
        use crate::models::ApiTimeInForce;

        let default = ApiTimeInForce::default();
        assert_eq!(default, ApiTimeInForce::Gtc);
    }

    #[test]
    fn test_limit_order_status_serialization() {
        use crate::models::LimitOrderStatus;

        let accepted = LimitOrderStatus::Accepted;
        let json = serde_json::to_string(&accepted).unwrap();
        assert_eq!(json, "\"accepted\"");

        let filled = LimitOrderStatus::Filled;
        let json = serde_json::to_string(&filled).unwrap();
        assert_eq!(json, "\"filled\"");

        let partial = LimitOrderStatus::Partial;
        let json = serde_json::to_string(&partial).unwrap();
        assert_eq!(json, "\"partial\"");

        let rejected = LimitOrderStatus::Rejected;
        let json = serde_json::to_string(&rejected).unwrap();
        assert_eq!(json, "\"rejected\"");
    }

    #[test]
    fn test_add_order_request_with_tif() {
        use crate::models::{AddOrderRequest, ApiTimeInForce, OrderSide};

        // Test with explicit TIF
        let json = r#"{"side":"buy","price":100,"quantity":10,"time_in_force":"IOC"}"#;
        let request: AddOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.side, OrderSide::Buy);
        assert_eq!(request.price, 100);
        assert_eq!(request.quantity, 10);
        assert_eq!(request.time_in_force, Some(ApiTimeInForce::Ioc));
    }

    #[test]
    fn test_add_order_request_default_tif() {
        use crate::models::{AddOrderRequest, OrderSide};

        // Test without TIF (should default to None, which means GTC)
        let json = r#"{"side":"sell","price":200,"quantity":5}"#;
        let request: AddOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.side, OrderSide::Sell);
        assert_eq!(request.price, 200);
        assert_eq!(request.quantity, 5);
        assert_eq!(request.time_in_force, None);
    }

    #[test]
    fn test_add_order_request_with_expire_at() {
        use crate::models::{AddOrderRequest, ApiTimeInForce, OrderSide};

        let json = r#"{"side":"buy","price":100,"quantity":10,"time_in_force":"GTD","expire_at":"2024-12-31T16:00:00Z"}"#;
        let request: AddOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.side, OrderSide::Buy);
        assert_eq!(request.time_in_force, Some(ApiTimeInForce::Gtd));
        assert_eq!(request.expire_at, Some("2024-12-31T16:00:00Z".to_string()));
    }

    #[test]
    fn test_add_order_response_serialization() {
        use crate::models::{AddOrderResponse, LimitOrderStatus};

        let response = AddOrderResponse {
            order_id: "test-order-123".to_string(),
            status: LimitOrderStatus::Accepted,
            filled_quantity: 0,
            remaining_quantity: 100,
            message: "Order added successfully".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"order_id\":\"test-order-123\""));
        assert!(json.contains("\"status\":\"accepted\""));
        assert!(json.contains("\"filled_quantity\":0"));
        assert!(json.contains("\"remaining_quantity\":100"));
    }

    // ========================================================================
    // OHLC Tests
    // ========================================================================

    #[test]
    fn test_ohlc_interval_parsing() {
        use crate::models::OhlcInterval;

        assert_eq!(
            "1m".parse::<OhlcInterval>().unwrap(),
            OhlcInterval::OneMinute
        );
        assert_eq!(
            "5m".parse::<OhlcInterval>().unwrap(),
            OhlcInterval::FiveMinutes
        );
        assert_eq!(
            "15m".parse::<OhlcInterval>().unwrap(),
            OhlcInterval::FifteenMinutes
        );
        assert_eq!("1h".parse::<OhlcInterval>().unwrap(), OhlcInterval::OneHour);
        assert_eq!(
            "4h".parse::<OhlcInterval>().unwrap(),
            OhlcInterval::FourHours
        );
        assert_eq!("1d".parse::<OhlcInterval>().unwrap(), OhlcInterval::OneDay);

        assert!("invalid".parse::<OhlcInterval>().is_err());
    }

    #[test]
    fn test_ohlc_interval_serialization() {
        use crate::models::OhlcInterval;

        let interval = OhlcInterval::OneMinute;
        let json = serde_json::to_string(&interval).unwrap();
        assert_eq!(json, "\"1m\"");

        let interval = OhlcInterval::FourHours;
        let json = serde_json::to_string(&interval).unwrap();
        assert_eq!(json, "\"4h\"");
    }

    #[test]
    fn test_ohlc_interval_seconds() {
        use crate::models::OhlcInterval;

        assert_eq!(OhlcInterval::OneMinute.seconds(), 60);
        assert_eq!(OhlcInterval::FiveMinutes.seconds(), 300);
        assert_eq!(OhlcInterval::FifteenMinutes.seconds(), 900);
        assert_eq!(OhlcInterval::OneHour.seconds(), 3600);
        assert_eq!(OhlcInterval::FourHours.seconds(), 14400);
        assert_eq!(OhlcInterval::OneDay.seconds(), 86400);
    }

    #[test]
    fn test_ohlc_bar_creation() {
        use crate::models::OhlcBar;

        let bar = OhlcBar::new(1704067200, 500, 100);
        assert_eq!(bar.timestamp, 1704067200);
        assert_eq!(bar.open, 500);
        assert_eq!(bar.high, 500);
        assert_eq!(bar.low, 500);
        assert_eq!(bar.close, 500);
        assert_eq!(bar.volume, 100);
        assert_eq!(bar.trade_count, 1);
    }

    #[test]
    fn test_ohlc_bar_update() {
        use crate::models::OhlcBar;

        let mut bar = OhlcBar::new(1704067200, 500, 100);

        // Update with higher price
        bar.update(520, 50);
        assert_eq!(bar.high, 520);
        assert_eq!(bar.low, 500);
        assert_eq!(bar.close, 520);
        assert_eq!(bar.volume, 150);
        assert_eq!(bar.trade_count, 2);

        // Update with lower price
        bar.update(480, 75);
        assert_eq!(bar.high, 520);
        assert_eq!(bar.low, 480);
        assert_eq!(bar.close, 480);
        assert_eq!(bar.volume, 225);
        assert_eq!(bar.trade_count, 3);
    }

    #[test]
    fn test_ohlc_query_deserialization() {
        use crate::models::OhlcQuery;

        let json = r#"{"interval":"1m","from":1704067200,"to":1704153600,"limit":100}"#;
        let query: OhlcQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.interval, "1m");
        assert_eq!(query.from, Some(1704067200));
        assert_eq!(query.to, Some(1704153600));
        assert_eq!(query.limit, Some(100));
    }

    #[test]
    fn test_ohlc_query_minimal() {
        use crate::models::OhlcQuery;

        let json = r#"{"interval":"5m"}"#;
        let query: OhlcQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.interval, "5m");
        assert_eq!(query.from, None);
        assert_eq!(query.to, None);
        assert_eq!(query.limit, None);
    }

    #[test]
    fn test_ohlc_response_serialization() {
        use crate::models::{OhlcBar, OhlcResponse};

        let response = OhlcResponse {
            symbol: "AAPL-20251231-150-C".to_string(),
            interval: "1m".to_string(),
            bars: vec![
                OhlcBar::new(1704067200, 500, 100),
                OhlcBar::new(1704067260, 510, 50),
            ],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"symbol\":\"AAPL-20251231-150-C\""));
        assert!(json.contains("\"interval\":\"1m\""));
        assert!(json.contains("\"bars\":["));
    }

    #[tokio::test]
    async fn test_get_ohlc_empty() {
        let state = create_test_state();

        let result = get_ohlc(
            State(state.clone()),
            Path((
                "OHLC1".to_string(),
                "20251231".to_string(),
                100,
                "call".to_string(),
            )),
            Query(OhlcQuery {
                interval: "1m".to_string(),
                from: None,
                to: None,
                limit: None,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.symbol, "OHLC1-20251231-100-C");
        assert_eq!(response.interval, "1m");
        assert!(response.bars.is_empty());
    }

    #[tokio::test]
    async fn test_get_ohlc_with_data() {
        let state = create_test_state();

        // Record some trades
        let symbol = "OHLC2-20251231-100-C";
        state
            .ohlc_aggregator
            .record_trade(symbol, 1704067200000, 500, 100);
        state
            .ohlc_aggregator
            .record_trade(symbol, 1704067210000, 510, 50);

        let result = get_ohlc(
            State(state.clone()),
            Path((
                "OHLC2".to_string(),
                "20251231".to_string(),
                100,
                "call".to_string(),
            )),
            Query(OhlcQuery {
                interval: "1m".to_string(),
                from: None,
                to: None,
                limit: None,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.bars.len(), 1);
        assert_eq!(response.bars[0].open, 500);
        assert_eq!(response.bars[0].close, 510);
        assert_eq!(response.bars[0].volume, 150);
    }

    #[tokio::test]
    async fn test_get_ohlc_invalid_interval() {
        let state = create_test_state();

        let result = get_ohlc(
            State(state.clone()),
            Path((
                "OHLC3".to_string(),
                "20251231".to_string(),
                100,
                "call".to_string(),
            )),
            Query(OhlcQuery {
                interval: "invalid".to_string(),
                from: None,
                to: None,
                limit: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }

    // ========================================================================
    // Order Modification Tests
    // ========================================================================

    #[test]
    fn test_modify_order_request_deserialization() {
        use crate::models::ModifyOrderRequest;

        // Both fields
        let json = r#"{"price":100,"quantity":50}"#;
        let request: ModifyOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.price, Some(100));
        assert_eq!(request.quantity, Some(50));

        // Only price
        let json = r#"{"price":200}"#;
        let request: ModifyOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.price, Some(200));
        assert_eq!(request.quantity, None);

        // Only quantity
        let json = r#"{"quantity":75}"#;
        let request: ModifyOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.price, None);
        assert_eq!(request.quantity, Some(75));

        // Empty (valid JSON but will fail validation in handler)
        let json = r#"{}"#;
        let request: ModifyOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.price, None);
        assert_eq!(request.quantity, None);
    }

    #[test]
    fn test_modify_order_status_serialization() {
        use crate::models::ModifyOrderStatus;

        let modified = ModifyOrderStatus::Modified;
        let json = serde_json::to_string(&modified).unwrap();
        assert_eq!(json, "\"modified\"");

        let rejected = ModifyOrderStatus::Rejected;
        let json = serde_json::to_string(&rejected).unwrap();
        assert_eq!(json, "\"rejected\"");
    }

    #[test]
    fn test_modify_order_response_serialization() {
        use crate::models::{ModifyOrderResponse, ModifyOrderStatus};

        let response = ModifyOrderResponse {
            order_id: "test-order-123".to_string(),
            status: ModifyOrderStatus::Modified,
            new_price: Some(100),
            new_quantity: Some(50),
            priority_changed: true,
            message: "Order modified successfully".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"order_id\":\"test-order-123\""));
        assert!(json.contains("\"status\":\"modified\""));
        assert!(json.contains("\"new_price\":100"));
        assert!(json.contains("\"new_quantity\":50"));
        assert!(json.contains("\"priority_changed\":true"));
    }

    #[tokio::test]
    async fn test_modify_order_no_changes() {
        let state = create_test_state();

        // Create underlying, expiration, strike
        let underlying = state.manager.get_or_create("MOD1");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        let exp_book = underlying.get_or_create_expiration(exp);
        drop(exp_book.get_or_create_strike(100));

        // Try to modify with no price or quantity - should fail validation
        let result = modify_order(
            State(state.clone()),
            Path((
                "MOD1".to_string(),
                "20251231".to_string(),
                100,
                "call".to_string(),
                "12345".to_string(),
            )),
            Json(ModifyOrderRequest {
                price: None,
                quantity: None,
            }),
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(ApiError::InvalidRequest(msg)) => {
                assert!(msg.contains("At least one"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_modify_order_underlying_not_found() {
        let state = create_test_state();

        let result = modify_order(
            State(state.clone()),
            Path((
                "NONEXISTENT".to_string(),
                "20251231".to_string(),
                100,
                "call".to_string(),
                "12345".to_string(),
            )),
            Json(ModifyOrderRequest {
                price: Some(100),
                quantity: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_modify_order_invalid_style() {
        let state = create_test_state();

        // Create underlying
        state.manager.get_or_create("MOD2");

        let result = modify_order(
            State(state.clone()),
            Path((
                "MOD2".to_string(),
                "20251231".to_string(),
                100,
                "invalid".to_string(),
                "12345".to_string(),
            )),
            Json(ModifyOrderRequest {
                price: Some(100),
                quantity: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }

    // ========================================================================
    // Bulk Order Operations Tests
    // ========================================================================

    #[test]
    fn test_bulk_order_request_deserialization() {
        use crate::models::BulkOrderRequest;

        let json = r#"{
            "orders": [
                {
                    "underlying": "AAPL",
                    "expiration": "20240329",
                    "strike": 15000,
                    "style": "call",
                    "side": "buy",
                    "price": 150,
                    "quantity": 10
                }
            ],
            "atomic": true
        }"#;

        let request: BulkOrderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.orders.len(), 1);
        assert!(request.atomic);
        assert_eq!(request.orders[0].underlying, "AAPL");
        assert_eq!(request.orders[0].strike, 15000);
    }

    #[test]
    fn test_bulk_order_request_default_atomic() {
        use crate::models::BulkOrderRequest;

        let json = r#"{
            "orders": [
                {
                    "underlying": "AAPL",
                    "expiration": "20240329",
                    "strike": 15000,
                    "style": "call",
                    "side": "buy",
                    "price": 150,
                    "quantity": 10
                }
            ]
        }"#;

        let request: BulkOrderRequest = serde_json::from_str(json).unwrap();
        assert!(!request.atomic); // Default is false
    }

    #[test]
    fn test_bulk_order_response_serialization() {
        use crate::models::{BulkOrderResponse, BulkOrderResultItem, BulkOrderStatus};

        let response = BulkOrderResponse {
            success_count: 2,
            failure_count: 1,
            results: vec![
                BulkOrderResultItem {
                    index: 0,
                    order_id: Some("order-1".to_string()),
                    status: BulkOrderStatus::Accepted,
                    error: None,
                },
                BulkOrderResultItem {
                    index: 1,
                    order_id: None,
                    status: BulkOrderStatus::Rejected,
                    error: Some("Invalid style".to_string()),
                },
            ],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"success_count\":2"));
        assert!(json.contains("\"failure_count\":1"));
        assert!(json.contains("\"status\":\"accepted\""));
        assert!(json.contains("\"status\":\"rejected\""));
    }

    #[test]
    fn test_bulk_cancel_request_deserialization() {
        use crate::models::BulkCancelRequest;

        let json = r#"{"order_ids": ["order-1", "order-2", "order-3"]}"#;
        let request: BulkCancelRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.order_ids.len(), 3);
        assert_eq!(request.order_ids[0], "order-1");
    }

    #[test]
    fn test_bulk_cancel_response_serialization() {
        use crate::models::{BulkCancelResponse, BulkCancelResultItem};

        let response = BulkCancelResponse {
            success_count: 2,
            failure_count: 1,
            results: vec![
                BulkCancelResultItem {
                    order_id: "order-1".to_string(),
                    canceled: true,
                    error: None,
                },
                BulkCancelResultItem {
                    order_id: "order-2".to_string(),
                    canceled: false,
                    error: Some("Order not found".to_string()),
                },
            ],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"success_count\":2"));
        assert!(json.contains("\"canceled\":true"));
        assert!(json.contains("\"canceled\":false"));
    }

    #[test]
    fn test_cancel_all_query_defaults() {
        use crate::models::CancelAllQuery;

        // Test that CancelAllQuery can be constructed with all None values
        let query = CancelAllQuery {
            underlying: None,
            expiration: None,
            side: None,
            style: None,
        };
        assert!(query.underlying.is_none());
        assert!(query.expiration.is_none());
        assert!(query.side.is_none());
        assert!(query.style.is_none());

        // Test with some values
        let query = CancelAllQuery {
            underlying: Some("AAPL".to_string()),
            expiration: Some("20240329".to_string()),
            side: Some("buy".to_string()),
            style: Some("call".to_string()),
        };
        assert_eq!(query.underlying, Some("AAPL".to_string()));
        assert_eq!(query.expiration, Some("20240329".to_string()));
        assert_eq!(query.side, Some("buy".to_string()));
        assert_eq!(query.style, Some("call".to_string()));
    }

    #[test]
    fn test_cancel_all_response_serialization() {
        use crate::models::CancelAllResponse;

        let response = CancelAllResponse {
            canceled_count: 42,
            failed_count: 3,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"canceled_count\":42"));
        assert!(json.contains("\"failed_count\":3"));
    }

    #[tokio::test]
    async fn test_bulk_submit_orders_empty() {
        let state = create_test_state();

        let result = bulk_submit_orders(
            State(state.clone()),
            Json(BulkOrderRequest {
                orders: vec![],
                atomic: false,
            }),
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(ApiError::InvalidRequest(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_bulk_cancel_orders_empty() {
        let state = create_test_state();

        let result = bulk_cancel_orders(
            State(state.clone()),
            Json(BulkCancelRequest { order_ids: vec![] }),
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(ApiError::InvalidRequest(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_bulk_cancel_orders_not_found() {
        let state = create_test_state();

        let result = bulk_cancel_orders(
            State(state.clone()),
            Json(BulkCancelRequest {
                order_ids: vec!["nonexistent-order".to_string()],
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.success_count, 0);
        assert_eq!(response.failure_count, 1);
        assert!(!response.results[0].canceled);
    }

    #[tokio::test]
    async fn test_cancel_all_orders_no_filters() {
        let state = create_test_state();

        let result = cancel_all_orders(
            State(state.clone()),
            Query(CancelAllQuery {
                underlying: None,
                expiration: None,
                side: None,
                style: None,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        // No orders to cancel in empty state
        assert_eq!(response.canceled_count, 0);
        assert_eq!(response.failed_count, 0);
    }

    #[tokio::test]
    async fn test_cancel_all_orders_invalid_side() {
        let state = create_test_state();

        let result = cancel_all_orders(
            State(state.clone()),
            Query(CancelAllQuery {
                underlying: None,
                expiration: None,
                side: Some("invalid".to_string()),
                style: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_all_orders_invalid_style() {
        let state = create_test_state();

        let result = cancel_all_orders(
            State(state.clone()),
            Query(CancelAllQuery {
                underlying: None,
                expiration: None,
                side: None,
                style: Some("invalid".to_string()),
            }),
        )
        .await;

        assert!(result.is_err());
    }

    // ========================================================================
    // Option Chain Matrix Tests
    // ========================================================================

    #[test]
    fn test_chain_query_defaults() {
        use crate::models::ChainQuery;

        let query = ChainQuery {
            min_strike: None,
            max_strike: None,
        };
        assert!(query.min_strike.is_none());
        assert!(query.max_strike.is_none());

        let query = ChainQuery {
            min_strike: Some(14000),
            max_strike: Some(16000),
        };
        assert_eq!(query.min_strike, Some(14000));
        assert_eq!(query.max_strike, Some(16000));
    }

    #[test]
    fn test_option_quote_data_default() {
        use crate::models::OptionQuoteData;

        let quote = OptionQuoteData::default();
        assert!(quote.bid.is_none());
        assert!(quote.ask.is_none());
        assert_eq!(quote.bid_size, 0);
        assert_eq!(quote.ask_size, 0);
        assert!(quote.last_trade.is_none());
        assert_eq!(quote.volume, 0);
        assert_eq!(quote.open_interest, 0);
        assert!(quote.delta.is_none());
        assert!(quote.gamma.is_none());
        assert!(quote.theta.is_none());
        assert!(quote.vega.is_none());
        assert!(quote.iv.is_none());
    }

    #[test]
    fn test_option_quote_data_serialization() {
        use crate::models::OptionQuoteData;

        let quote = OptionQuoteData {
            bid: Some(100),
            ask: Some(105),
            bid_size: 50,
            ask_size: 30,
            last_trade: Some(102),
            volume: 1000,
            open_interest: 5000,
            delta: Some(0.65),
            gamma: None,
            theta: None,
            vega: None,
            iv: Some(0.32),
        };

        let json = serde_json::to_string(&quote).unwrap();
        assert!(json.contains("\"bid\":100"));
        assert!(json.contains("\"ask\":105"));
        assert!(json.contains("\"bid_size\":50"));
        assert!(json.contains("\"delta\":0.65"));
        assert!(json.contains("\"iv\":0.32"));
        // gamma, theta, vega should be skipped (None)
        assert!(!json.contains("\"gamma\""));
        assert!(!json.contains("\"theta\""));
        assert!(!json.contains("\"vega\""));
    }

    #[test]
    fn test_chain_strike_row_serialization() {
        use crate::models::{ChainStrikeRow, OptionQuoteData};

        let row = ChainStrikeRow {
            strike: 15000,
            call: OptionQuoteData {
                bid: Some(500),
                ask: Some(510),
                ..Default::default()
            },
            put: OptionQuoteData {
                bid: Some(50),
                ask: Some(55),
                ..Default::default()
            },
        };

        let json = serde_json::to_string(&row).unwrap();
        assert!(json.contains("\"strike\":15000"));
        assert!(json.contains("\"call\""));
        assert!(json.contains("\"put\""));
    }

    #[test]
    fn test_option_chain_response_serialization() {
        use crate::models::{ChainStrikeRow, OptionChainResponse, OptionQuoteData};

        let response = OptionChainResponse {
            underlying: "AAPL".to_string(),
            expiration: "20240329".to_string(),
            spot_price: Some(15000),
            atm_strike: Some(15000),
            chain: vec![ChainStrikeRow {
                strike: 15000,
                call: OptionQuoteData::default(),
                put: OptionQuoteData::default(),
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"underlying\":\"AAPL\""));
        assert!(json.contains("\"expiration\":\"20240329\""));
        assert!(json.contains("\"spot_price\":15000"));
        assert!(json.contains("\"atm_strike\":15000"));
        assert!(json.contains("\"chain\""));
    }

    #[tokio::test]
    async fn test_get_option_chain_underlying_not_found() {
        let state = create_test_state();

        let result = get_option_chain(
            State(state.clone()),
            Path(("NONEXISTENT".to_string(), "20251231".to_string())),
            Query(ChainQuery {
                min_strike: None,
                max_strike: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_option_chain_empty() {
        let state = create_test_state();

        // Create underlying and expiration but no strikes
        let underlying = state.manager.get_or_create("CHAIN1");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        underlying.get_or_create_expiration(exp);

        let result = get_option_chain(
            State(state.clone()),
            Path(("CHAIN1".to_string(), "20251231".to_string())),
            Query(ChainQuery {
                min_strike: None,
                max_strike: None,
            }),
        )
        .await;

        // Should fail because expiration string doesn't match
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_option_chain_with_strikes() {
        let state = create_test_state();

        // Create underlying, expiration, and strikes
        let underlying = state.manager.get_or_create("CHAIN2");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        let exp_book = underlying.get_or_create_expiration(exp);
        exp_book.get_or_create_strike(14000);
        exp_book.get_or_create_strike(15000);
        exp_book.get_or_create_strike(16000);

        // Get the expiration string
        let exp_str = exp.get_date().unwrap().format("%Y%m%d").to_string();

        let result = get_option_chain(
            State(state.clone()),
            Path(("CHAIN2".to_string(), exp_str)),
            Query(ChainQuery {
                min_strike: None,
                max_strike: None,
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.underlying, "CHAIN2");
        assert_eq!(response.chain.len(), 3);
        // Strikes should be sorted
        assert_eq!(response.chain[0].strike, 14000);
        assert_eq!(response.chain[1].strike, 15000);
        assert_eq!(response.chain[2].strike, 16000);
    }

    #[tokio::test]
    async fn test_get_option_chain_with_strike_filter() {
        let state = create_test_state();

        // Create underlying, expiration, and strikes
        let underlying = state.manager.get_or_create("CHAIN3");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        let exp_book = underlying.get_or_create_expiration(exp);
        exp_book.get_or_create_strike(14000);
        exp_book.get_or_create_strike(15000);
        exp_book.get_or_create_strike(16000);
        exp_book.get_or_create_strike(17000);

        // Get the expiration string
        let exp_str = exp.get_date().unwrap().format("%Y%m%d").to_string();

        let result = get_option_chain(
            State(state.clone()),
            Path(("CHAIN3".to_string(), exp_str)),
            Query(ChainQuery {
                min_strike: Some(14500),
                max_strike: Some(16500),
            }),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        // Only strikes 15000 and 16000 should be included
        assert_eq!(response.chain.len(), 2);
        assert_eq!(response.chain[0].strike, 15000);
        assert_eq!(response.chain[1].strike, 16000);
    }

    // ========================================================================
    // Greeks Tests
    // ========================================================================

    #[test]
    fn test_greeks_data_default() {
        use crate::models::GreeksData;

        let greeks = GreeksData::default();
        assert_eq!(greeks.delta, 0.0);
        assert_eq!(greeks.gamma, 0.0);
        assert_eq!(greeks.theta, 0.0);
        assert_eq!(greeks.vega, 0.0);
        assert_eq!(greeks.rho, 0.0);
        assert!(greeks.vanna.is_none());
        assert!(greeks.vomma.is_none());
        assert!(greeks.charm.is_none());
        assert!(greeks.color.is_none());
    }

    #[test]
    fn test_greeks_data_serialization() {
        use crate::models::GreeksData;

        let greeks = GreeksData {
            delta: 0.65,
            gamma: 0.02,
            theta: -0.15,
            vega: 0.25,
            rho: 0.10,
            vanna: Some(0.01),
            vomma: Some(0.005),
            charm: None,
            color: None,
        };

        let json = serde_json::to_string(&greeks).unwrap();
        assert!(json.contains("\"delta\":0.65"));
        assert!(json.contains("\"gamma\":0.02"));
        assert!(json.contains("\"theta\":-0.15"));
        assert!(json.contains("\"vega\":0.25"));
        assert!(json.contains("\"rho\":0.1"));
        assert!(json.contains("\"vanna\":0.01"));
        assert!(json.contains("\"vomma\":0.005"));
        // charm and color should be skipped (None)
        assert!(!json.contains("\"charm\""));
        assert!(!json.contains("\"color\""));
    }

    #[test]
    fn test_greeks_response_serialization() {
        use crate::models::{GreeksData, GreeksResponse};

        let response = GreeksResponse {
            symbol: "AAPL-20240329-15000-C".to_string(),
            greeks: GreeksData {
                delta: 0.65,
                gamma: 0.02,
                theta: -0.15,
                vega: 0.25,
                rho: 0.10,
                vanna: None,
                vomma: None,
                charm: None,
                color: None,
            },
            iv: 0.32,
            theoretical_value: 525.0,
            timestamp_ms: 1709123456789,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"symbol\":\"AAPL-20240329-15000-C\""));
        assert!(json.contains("\"greeks\""));
        assert!(json.contains("\"iv\":0.32"));
        assert!(json.contains("\"theoretical_value\":525.0"));
        assert!(json.contains("\"timestamp_ms\":1709123456789"));
    }

    #[tokio::test]
    async fn test_get_option_greeks_underlying_not_found() {
        let state = create_test_state();

        let result = get_option_greeks(
            State(state.clone()),
            Path((
                "NONEXISTENT".to_string(),
                "20251231".to_string(),
                15000,
                "call".to_string(),
            )),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_option_greeks_invalid_style() {
        let state = create_test_state();

        // Create underlying
        state.manager.get_or_create("GREEKS1");

        let result = get_option_greeks(
            State(state.clone()),
            Path((
                "GREEKS1".to_string(),
                "20251231".to_string(),
                15000,
                "invalid".to_string(),
            )),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_option_greeks_success() {
        let state = create_test_state();

        // Create underlying, expiration, and strike
        let underlying = state.manager.get_or_create("GREEKS2");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        let exp_book = underlying.get_or_create_expiration(exp);
        exp_book.get_or_create_strike(15000);

        // Get the expiration string
        let exp_str = exp.get_date().unwrap().format("%Y%m%d").to_string();

        let result = get_option_greeks(
            State(state.clone()),
            Path(("GREEKS2".to_string(), exp_str, 15000, "call".to_string())),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.symbol.contains("GREEKS2"));
        assert!(response.symbol.contains("15000"));
        assert!(response.symbol.contains("C"));
        assert_eq!(response.iv, 0.30); // Default IV
        // Greeks should have reasonable values
        assert!(response.greeks.delta >= -1.0 && response.greeks.delta <= 1.0);
    }

    #[tokio::test]
    async fn test_get_option_greeks_put() {
        let state = create_test_state();

        // Create underlying, expiration, and strike
        let underlying = state.manager.get_or_create("GREEKS3");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        let exp_book = underlying.get_or_create_expiration(exp);
        exp_book.get_or_create_strike(15000);

        // Get the expiration string
        let exp_str = exp.get_date().unwrap().format("%Y%m%d").to_string();

        let result = get_option_greeks(
            State(state.clone()),
            Path(("GREEKS3".to_string(), exp_str, 15000, "put".to_string())),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.symbol.contains("P"));
    }

    // ========================================================================
    // Volatility Surface Tests
    // ========================================================================

    #[test]
    fn test_strike_iv_default() {
        use crate::models::StrikeIV;

        let iv = StrikeIV::default();
        assert!(iv.call_iv.is_none());
        assert!(iv.put_iv.is_none());
    }

    #[test]
    fn test_strike_iv_serialization() {
        use crate::models::StrikeIV;

        let iv = StrikeIV {
            call_iv: Some(0.35),
            put_iv: Some(0.34),
        };

        let json = serde_json::to_string(&iv).unwrap();
        assert!(json.contains("\"call_iv\":0.35"));
        assert!(json.contains("\"put_iv\":0.34"));

        // Test with None values - should be skipped
        let iv_partial = StrikeIV {
            call_iv: Some(0.30),
            put_iv: None,
        };
        let json_partial = serde_json::to_string(&iv_partial).unwrap();
        assert!(json_partial.contains("\"call_iv\":0.3"));
        assert!(!json_partial.contains("\"put_iv\""));
    }

    #[test]
    fn test_atm_term_structure_point_serialization() {
        use crate::models::ATMTermStructurePoint;

        let point = ATMTermStructurePoint {
            expiration: "20240329".to_string(),
            days: 30,
            iv: 0.30,
        };

        let json = serde_json::to_string(&point).unwrap();
        assert!(json.contains("\"expiration\":\"20240329\""));
        assert!(json.contains("\"days\":30"));
        assert!(json.contains("\"iv\":0.3"));
    }

    #[test]
    fn test_volatility_surface_response_serialization() {
        use crate::models::{ATMTermStructurePoint, StrikeIV, VolatilitySurfaceResponse};
        use std::collections::HashMap;

        let mut surface: HashMap<String, HashMap<u64, StrikeIV>> = HashMap::new();
        let mut exp_surface: HashMap<u64, StrikeIV> = HashMap::new();
        exp_surface.insert(
            15000,
            StrikeIV {
                call_iv: Some(0.30),
                put_iv: Some(0.31),
            },
        );
        surface.insert("20240329".to_string(), exp_surface);

        let response = VolatilitySurfaceResponse {
            underlying: "AAPL".to_string(),
            spot_price: Some(15000),
            timestamp_ms: 1709123456789,
            expirations: vec!["20240329".to_string()],
            strikes: vec![14000, 15000, 16000],
            surface,
            atm_term_structure: vec![ATMTermStructurePoint {
                expiration: "20240329".to_string(),
                days: 30,
                iv: 0.30,
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"underlying\":\"AAPL\""));
        assert!(json.contains("\"spot_price\":15000"));
        assert!(json.contains("\"expirations\""));
        assert!(json.contains("\"strikes\""));
        assert!(json.contains("\"surface\""));
        assert!(json.contains("\"atm_term_structure\""));
    }

    #[tokio::test]
    async fn test_get_volatility_surface_underlying_not_found() {
        let state = create_test_state();

        let result =
            get_volatility_surface(State(state.clone()), Path("NONEXISTENT".to_string())).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_volatility_surface_empty() {
        let state = create_test_state();

        // Create underlying with no expirations
        state.manager.get_or_create("VOLSURF1");

        let result =
            get_volatility_surface(State(state.clone()), Path("VOLSURF1".to_string())).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.underlying, "VOLSURF1");
        assert!(response.expirations.is_empty());
        assert!(response.strikes.is_empty());
    }

    #[tokio::test]
    async fn test_get_volatility_surface_with_data() {
        let state = create_test_state();

        // Create underlying, expiration, and strikes
        let underlying = state.manager.get_or_create("VOLSURF2");
        let exp = ExpirationDate::Days(optionstratlib::prelude::Positive::new(30.0).unwrap());
        let exp_book = underlying.get_or_create_expiration(exp);
        exp_book.get_or_create_strike(14000);
        exp_book.get_or_create_strike(15000);
        exp_book.get_or_create_strike(16000);

        let result =
            get_volatility_surface(State(state.clone()), Path("VOLSURF2".to_string())).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.underlying, "VOLSURF2");
        assert_eq!(response.expirations.len(), 1);
        assert_eq!(response.strikes.len(), 3);
        // Surface should have data for the expiration
        assert_eq!(response.surface.len(), 1);
    }

    #[test]
    fn test_estimate_iv() {
        // Test ATM option
        let iv_atm = estimate_iv(Some(15000), 15000, 30, true);
        assert!((0.30..=0.35).contains(&iv_atm));

        // Test OTM call
        let iv_otm_call = estimate_iv(Some(15000), 16000, 30, true);
        assert!(iv_otm_call > iv_atm); // Should have smile adjustment

        // Test OTM put (should have skew)
        let iv_otm_put = estimate_iv(Some(15000), 14000, 30, false);
        assert!(iv_otm_put > iv_atm); // Should have smile + skew

        // Test without spot price
        let iv_no_spot = estimate_iv(None, 15000, 30, true);
        assert!(iv_no_spot >= 0.30);
    }

    #[test]
    fn test_calculate_mid_price() {
        use option_chain_orderbook::orderbook::Quote;

        // Both bid and ask (bid_price, bid_size, ask_price, ask_size, last_trade)
        let quote1 = Quote::new(Some(100), 10, Some(110), 10, 0);
        assert_eq!(calculate_mid_price(&quote1), Some(105));

        // Only bid
        let quote2 = Quote::new(Some(100), 10, None, 0, 0);
        assert_eq!(calculate_mid_price(&quote2), Some(100));

        // Only ask
        let quote3 = Quote::new(None, 0, Some(110), 10, 0);
        assert_eq!(calculate_mid_price(&quote3), Some(110));

        // Neither
        let quote4 = Quote::new(None, 0, None, 0, 0);
        assert_eq!(calculate_mid_price(&quote4), None);
    }
}

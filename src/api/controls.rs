//! Control and price API handlers.

use crate::db::{InsertPriceRequest, UpdateParametersRequest};
use crate::error::ApiError;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[cfg(test)]
mod tests;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Response for system control status.
#[derive(Debug, Serialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
pub struct KillSwitchResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
    /// Current master enabled state.
    pub master_enabled: bool,
}

/// Response for parameter update.
#[derive(Debug, Serialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
pub struct InstrumentToggleResponse {
    /// Whether the operation was successful.
    pub success: bool,
    /// Symbol that was toggled.
    pub symbol: String,
    /// New enabled state.
    pub enabled: bool,
}

/// Response for price insertion.
#[derive(Debug, Serialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
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

/// Instrument status.
#[derive(Debug, Serialize, ToSchema)]
pub struct InstrumentStatus {
    /// Symbol.
    pub symbol: String,
    /// Whether quoting is enabled.
    pub quoting_enabled: bool,
    /// Current price (if available).
    pub current_price: Option<f64>,
}

/// Response for listing instruments.
#[derive(Debug, Serialize, ToSchema)]
pub struct InstrumentsListResponse {
    /// List of instruments.
    pub instruments: Vec<InstrumentStatus>,
}

// ============================================================================
// Control Handlers
// ============================================================================

/// Get current system control status.
#[utoipa::path(
    get,
    path = "/api/v1/controls",
    responses(
        (status = 200, description = "Current control status", body = SystemControlResponse)
    ),
    tag = "Controls"
)]
pub async fn get_controls(State(state): State<Arc<AppState>>) -> Json<SystemControlResponse> {
    let config = state.market_maker.get_config();

    Json(SystemControlResponse {
        master_enabled: config.enabled,
        spread_multiplier: config.spread_multiplier,
        size_scalar: config.size_scalar,
        directional_skew: config.directional_skew,
    })
}

/// Activate the kill switch (disable all quoting).
#[utoipa::path(
    post,
    path = "/api/v1/controls/kill-switch",
    responses(
        (status = 200, description = "Kill switch activated", body = KillSwitchResponse)
    ),
    tag = "Controls"
)]
pub async fn kill_switch(State(state): State<Arc<AppState>>) -> Json<KillSwitchResponse> {
    let was_enabled = state.market_maker.is_enabled();

    if was_enabled {
        state.market_maker.set_enabled(false);
    }

    Json(KillSwitchResponse {
        success: true,
        message: if was_enabled {
            "Kill switch activated - all quoting disabled".to_string()
        } else {
            "Quoting was already disabled".to_string()
        },
        master_enabled: false,
    })
}

/// Enable quoting (deactivate kill switch).
#[utoipa::path(
    post,
    path = "/api/v1/controls/enable",
    responses(
        (status = 200, description = "Quoting enabled", body = KillSwitchResponse)
    ),
    tag = "Controls"
)]
pub async fn enable_quoting(State(state): State<Arc<AppState>>) -> Json<KillSwitchResponse> {
    state.market_maker.set_enabled(true);

    Json(KillSwitchResponse {
        success: true,
        message: "Quoting enabled".to_string(),
        master_enabled: true,
    })
}

/// Update global parameters.
#[utoipa::path(
    post,
    path = "/api/v1/controls/parameters",
    request_body = UpdateParametersRequest,
    responses(
        (status = 200, description = "Parameters updated", body = UpdateParametersResponse)
    ),
    tag = "Controls"
)]
pub async fn update_parameters(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateParametersRequest>,
) -> Json<UpdateParametersResponse> {
    if let Some(spread) = body.spread_multiplier {
        state.market_maker.set_spread_multiplier(spread);
    }

    if let Some(size) = body.size_scalar {
        state.market_maker.set_size_scalar(size / 100.0); // Convert from percentage
    }

    if let Some(skew) = body.directional_skew {
        state.market_maker.set_directional_skew(skew);
    }

    let config = state.market_maker.get_config();

    Json(UpdateParametersResponse {
        success: true,
        spread_multiplier: config.spread_multiplier,
        size_scalar: config.size_scalar * 100.0, // Convert back to percentage
        directional_skew: config.directional_skew,
    })
}

/// Toggle quoting for a specific instrument.
#[utoipa::path(
    post,
    path = "/api/v1/controls/instrument/{symbol}/toggle",
    params(
        ("symbol" = String, Path, description = "Instrument symbol")
    ),
    responses(
        (status = 200, description = "Instrument toggled", body = InstrumentToggleResponse)
    ),
    tag = "Controls"
)]
pub async fn toggle_instrument(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
) -> Json<InstrumentToggleResponse> {
    let was_enabled = state.market_maker.is_symbol_enabled(&symbol);
    state.market_maker.set_symbol_enabled(&symbol, !was_enabled);

    Json(InstrumentToggleResponse {
        success: true,
        symbol,
        enabled: !was_enabled,
    })
}

/// List all instruments with their status.
#[utoipa::path(
    get,
    path = "/api/v1/controls/instruments",
    responses(
        (status = 200, description = "List of instruments", body = InstrumentsListResponse)
    ),
    tag = "Controls"
)]
pub async fn list_instruments(State(state): State<Arc<AppState>>) -> Json<InstrumentsListResponse> {
    let symbols = state.manager.underlying_symbols();

    let instruments: Vec<InstrumentStatus> = symbols
        .into_iter()
        .map(|symbol| {
            let enabled = state.market_maker.is_symbol_enabled(&symbol);
            let price = state
                .market_maker
                .get_price(&symbol)
                .map(|p| p as f64 / 100.0);

            InstrumentStatus {
                symbol,
                quoting_enabled: enabled,
                current_price: price,
            }
        })
        .collect();

    Json(InstrumentsListResponse { instruments })
}

// ============================================================================
// Price Handlers
// ============================================================================

/// Insert a new underlying price.
#[utoipa::path(
    post,
    path = "/api/v1/prices",
    request_body = InsertPriceRequest,
    responses(
        (status = 200, description = "Price inserted", body = InsertPriceResponse),
        (status = 400, description = "Invalid request")
    ),
    tag = "Prices"
)]
pub async fn insert_price(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InsertPriceRequest>,
) -> Result<Json<InsertPriceResponse>, ApiError> {
    let price_cents = (body.price * 100.0).round() as i64;
    let timestamp = Utc::now();

    // Update the market maker with the new price
    state
        .market_maker
        .update_price(&body.symbol, price_cents as u64);

    // If we have a database, persist the price
    if let Some(ref db) = state.db {
        let bid_cents = body.bid.map(|b| (b * 100.0).round() as i64);
        let ask_cents = body.ask.map(|a| (a * 100.0).round() as i64);

        sqlx::query(
            r#"
            INSERT INTO underlying_prices (symbol, price_cents, bid_cents, ask_cents, volume, timestamp, source)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(&body.symbol)
        .bind(price_cents)
        .bind(bid_cents)
        .bind(ask_cents)
        .bind(body.volume)
        .bind(timestamp)
        .bind(&body.source)
        .execute(db.pool())
        .await
        .map_err(|e| ApiError::Database(e.to_string()))?;
    }

    Ok(Json(InsertPriceResponse {
        success: true,
        symbol: body.symbol,
        price_cents,
        timestamp: timestamp.to_rfc3339(),
    }))
}

/// Get the latest price for a symbol.
#[utoipa::path(
    get,
    path = "/api/v1/prices/{symbol}",
    params(
        ("symbol" = String, Path, description = "Underlying symbol")
    ),
    responses(
        (status = 200, description = "Latest price", body = LatestPriceResponse),
        (status = 404, description = "Symbol not found")
    ),
    tag = "Prices"
)]
pub async fn get_latest_price(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
) -> Result<Json<LatestPriceResponse>, ApiError> {
    // First check in-memory price
    if let Some(price_cents) = state.market_maker.get_price(&symbol) {
        return Ok(Json(LatestPriceResponse {
            symbol,
            price: price_cents as f64 / 100.0,
            bid: None,
            ask: None,
            volume: None,
            timestamp: Utc::now().to_rfc3339(),
        }));
    }

    // If we have a database, try to get from there
    if let Some(ref db) = state.db {
        type PriceRow = (
            i64,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            chrono::DateTime<Utc>,
        );
        let row: Option<PriceRow> = sqlx::query_as(
            r#"
                SELECT price_cents, bid_cents, ask_cents, volume, timestamp
                FROM underlying_prices
                WHERE symbol = $1
                ORDER BY timestamp DESC
                LIMIT 1
                "#,
        )
        .bind(&symbol)
        .fetch_optional(db.pool())
        .await
        .map_err(|e| ApiError::Database(e.to_string()))?;

        if let Some((price_cents, bid_cents, ask_cents, volume, timestamp)) = row {
            return Ok(Json(LatestPriceResponse {
                symbol,
                price: price_cents as f64 / 100.0,
                bid: bid_cents.map(|b| b as f64 / 100.0),
                ask: ask_cents.map(|a| a as f64 / 100.0),
                volume,
                timestamp: timestamp.to_rfc3339(),
            }));
        }
    }

    Err(ApiError::NotFound(format!(
        "No price found for symbol: {}",
        symbol
    )))
}

/// Get prices for all symbols.
#[utoipa::path(
    get,
    path = "/api/v1/prices",
    responses(
        (status = 200, description = "All latest prices", body = Vec<LatestPriceResponse>)
    ),
    tag = "Prices"
)]
pub async fn get_all_prices(State(state): State<Arc<AppState>>) -> Json<Vec<LatestPriceResponse>> {
    let symbols = state.manager.underlying_symbols();
    let timestamp = Utc::now().to_rfc3339();

    let prices: Vec<LatestPriceResponse> = symbols
        .into_iter()
        .filter_map(|symbol| {
            state
                .market_maker
                .get_price(&symbol)
                .map(|price_cents| LatestPriceResponse {
                    symbol,
                    price: price_cents as f64 / 100.0,
                    bid: None,
                    ask: None,
                    volume: None,
                    timestamp: timestamp.clone(),
                })
        })
        .collect();

    Json(prices)
}

//! Control and price API handlers.

use crate::db::{InsertPriceRequest, UpdateParametersRequest};
use crate::error::ApiError;
use crate::market_maker::{
    DIRECTIONAL_SKEW_MAX, DIRECTIONAL_SKEW_MIN, SIZE_SCALAR_MAX, SIZE_SCALAR_MIN,
    SPREAD_MULTIPLIER_MAX, SPREAD_MULTIPLIER_MIN, validate_control_value,
};
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
///
/// Every provided field is validated (finite and within its documented range)
/// BEFORE any value is applied, so a single bad field leaves the configuration
/// entirely unchanged. The control values are `f64` and `f64::clamp` returns
/// `NaN` for a `NaN` input, so a non-finite value must be rejected here rather
/// than slipping into the engine and poisoning quoting math.
///
/// # Errors
/// Returns [`ApiError::InvalidRequest`] (HTTP 400) when any provided field is
/// non-finite (`NaN` / infinite) or outside its documented range:
/// `spread_multiplier` ∈ [0.1, 10.0], `size_scalar` (the engine scalar, after
/// the percentage conversion) ∈ [0.0, 1.0], `directional_skew` ∈ [-1.0, 1.0].
#[utoipa::path(
    post,
    path = "/api/v1/controls/parameters",
    request_body = UpdateParametersRequest,
    responses(
        (status = 200, description = "Parameters updated", body = UpdateParametersResponse),
        (status = 400, description = "Invalid parameter value")
    ),
    tag = "Controls"
)]
pub async fn update_parameters(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateParametersRequest>,
) -> Result<Json<UpdateParametersResponse>, ApiError> {
    // Validate EVERY provided field up front, before mutating any engine state,
    // so an invalid field leaves the configuration entirely unchanged.
    let spread = body
        .spread_multiplier
        .map(|v| {
            validate_control_value(
                "spread_multiplier",
                v,
                SPREAD_MULTIPLIER_MIN,
                SPREAD_MULTIPLIER_MAX,
            )
        })
        .transpose()
        .map_err(ApiError::InvalidRequest)?;

    // The REST/WS wire contract carries `size_scalar` as a percentage; convert to
    // the engine scalar before validating against the documented [0.0, 1.0] range.
    let size = body
        .size_scalar
        .map(|v| validate_control_value("size_scalar", v / 100.0, SIZE_SCALAR_MIN, SIZE_SCALAR_MAX))
        .transpose()
        .map_err(ApiError::InvalidRequest)?;

    let skew = body
        .directional_skew
        .map(|v| {
            validate_control_value(
                "directional_skew",
                v,
                DIRECTIONAL_SKEW_MIN,
                DIRECTIONAL_SKEW_MAX,
            )
        })
        .transpose()
        .map_err(ApiError::InvalidRequest)?;

    // All provided values are valid: apply them (the engine still clamps finite
    // values to the documented range as the in-range coercion contract).
    if let Some(spread) = spread {
        state.market_maker.set_spread_multiplier(spread);
    }
    if let Some(size) = size {
        state.market_maker.set_size_scalar(size);
    }
    if let Some(skew) = skew {
        state.market_maker.set_directional_skew(skew);
    }

    let config = state.market_maker.get_config();

    Ok(Json(UpdateParametersResponse {
        success: true,
        spread_multiplier: config.spread_multiplier,
        size_scalar: config.size_scalar * 100.0, // Convert back to percentage
        directional_skew: config.directional_skew,
    }))
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

/// Maximum accepted monetary value, in dollars, for any inbound price field.
///
/// Inbound dollar amounts are multiplied by 100 to obtain integer cents. Capping
/// the input at one quadrillion dollars guarantees the resulting cents value
/// (`<= 1e17`) stays well within both `u64` and `i64` range, so the conversion
/// can never overflow or wrap. No real instrument price approaches this bound.
const MAX_PRICE_DOLLARS: f64 = 1e15;

/// Converts a dollar amount to integer cents with full validation.
///
/// Rejects non-finite (`NaN` / infinite), negative, and out-of-range values
/// (above [`MAX_PRICE_DOLLARS`]) so a bad `f64` can never wrap or saturate into a
/// corrupt cents value. The rounded result is only produced from a verified,
/// in-range value. The `field` name and the offending `value` are included in the
/// error message (no secrets).
///
/// # Errors
/// Returns [`ApiError::InvalidRequest`] when `value` is `NaN`, infinite,
/// negative, or greater than [`MAX_PRICE_DOLLARS`].
#[inline]
#[must_use = "the validated cents value (or rejection) must be handled"]
fn dollars_to_cents(field: &str, value: f64) -> Result<u64, ApiError> {
    if !value.is_finite() {
        return Err(ApiError::InvalidRequest(format!(
            "{field} must be a finite number, got {value}"
        )));
    }
    if value < 0.0 {
        return Err(ApiError::InvalidRequest(format!(
            "{field} must be non-negative, got {value}"
        )));
    }
    if value > MAX_PRICE_DOLLARS {
        return Err(ApiError::InvalidRequest(format!(
            "{field} exceeds maximum allowed value of {MAX_PRICE_DOLLARS}, got {value}"
        )));
    }
    // Safe: `value` is finite and in `[0, MAX_PRICE_DOLLARS]`, so the rounded
    // cents value lies in `[0, 1e17]`, well below `u64::MAX`. The cast cannot
    // overflow or wrap.
    Ok((value * 100.0).round() as u64)
}

/// Converts a validated cents value to the signed representation used by the
/// response DTO and the `price_cents` / `bid_cents` / `ask_cents` DB columns.
///
/// # Errors
/// Returns [`ApiError::InvalidRequest`] if the value does not fit in `i64`. With
/// inputs bounded by [`MAX_PRICE_DOLLARS`] this branch is unreachable in
/// practice, but the conversion stays checked rather than a silent `as` cast.
#[inline]
fn cents_to_i64(field: &str, cents: u64) -> Result<i64, ApiError> {
    i64::try_from(cents)
        .map_err(|_| ApiError::InvalidRequest(format!("{field} is too large to store: {cents}")))
}

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
    // Validate and convert EVERY monetary input up front, before mutating any
    // state. If any field is invalid we return 400 and touch neither the
    // in-memory market maker nor the database, so the two never diverge.
    let price_cents_u64 = dollars_to_cents("price", body.price)?;
    let bid_cents_u64 = body.bid.map(|b| dollars_to_cents("bid", b)).transpose()?;
    let ask_cents_u64 = body.ask.map(|a| dollars_to_cents("ask", a)).transpose()?;

    // Signed representations for the response DTO and the i64 DB columns.
    let price_cents = cents_to_i64("price", price_cents_u64)?;
    let bid_cents = bid_cents_u64.map(|c| cents_to_i64("bid", c)).transpose()?;
    let ask_cents = ask_cents_u64.map(|c| cents_to_i64("ask", c)).transpose()?;

    let timestamp = Utc::now();

    // All inputs are valid: update the in-memory market maker first...
    state
        .market_maker
        .update_price(&body.symbol, price_cents_u64);

    tracing::debug!(
        symbol = %body.symbol,
        price_cents = price_cents,
        "price inserted"
    );

    // ...then persist the same validated values so memory and DB agree.
    if let Some(ref db) = state.db {
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

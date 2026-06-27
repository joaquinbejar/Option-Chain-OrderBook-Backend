//! Market maker engine that coordinates quoting across all instruments.

use crate::db::DatabasePool;
use crate::market_maker::{OptionPricer, QuoteInput, Quoter};
use option_chain_orderbook::orderbook::UnderlyingOrderBookManager;
use optionstratlib::OptionStyle;
use orderbook_rs::{OrderId, Side};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Minimum accepted spread multiplier (dimensionless). Matches the engine clamp.
pub const SPREAD_MULTIPLIER_MIN: f64 = 0.1;
/// Maximum accepted spread multiplier (dimensionless). Matches the engine clamp.
pub const SPREAD_MULTIPLIER_MAX: f64 = 10.0;
/// Minimum accepted size scalar (fraction of full size). Matches the engine clamp.
pub const SIZE_SCALAR_MIN: f64 = 0.0;
/// Maximum accepted size scalar (fraction of full size). Matches the engine clamp.
pub const SIZE_SCALAR_MAX: f64 = 1.0;
/// Minimum accepted directional skew. Matches the engine clamp.
pub const DIRECTIONAL_SKEW_MIN: f64 = -1.0;
/// Maximum accepted directional skew. Matches the engine clamp.
pub const DIRECTIONAL_SKEW_MAX: f64 = 1.0;

/// Validates a market-maker control value is finite and within `[min, max]`.
///
/// The market-maker control parameters (`spread_multiplier`, `size_scalar`,
/// `directional_skew`) are `f64`. `f64::clamp` returns `NaN` when its input is
/// `NaN`, so a non-finite value would slip through the engine clamp and poison
/// quoting math. This helper is the single validation gate used by both the REST
/// (`POST /controls/parameters`) and WebSocket
/// (`set_spread` / `set_size` / `set_skew`) boundaries before a value ever reaches
/// the engine setters.
///
/// `RangeInclusive::contains` already rejects `NaN` (every comparison with `NaN`
/// is false) and both infinities (they fall outside any finite range), so a
/// single containment check covers finiteness and range together.
///
/// Returns the value unchanged when it is finite and within `[min, max]`,
/// otherwise an error message naming the `field`, the accepted range, and the
/// offending value (it contains no secrets).
///
/// # Errors
/// Returns a human-readable message when `value` is non-finite (`NaN` / infinite)
/// or outside the inclusive range `[min, max]`.
#[inline]
#[must_use = "the validation result must be handled"]
pub fn validate_control_value(field: &str, value: f64, min: f64, max: f64) -> Result<f64, String> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "{field} must be finite and within [{min}, {max}], got {value}"
        ))
    }
}

/// Market maker configuration.
#[derive(Debug, Clone)]
pub struct MarketMakerConfig {
    /// Whether the market maker is enabled.
    pub enabled: bool,
    /// Global spread multiplier.
    pub spread_multiplier: f64,
    /// Global size scalar (0.0 to 1.0).
    pub size_scalar: f64,
    /// Global directional skew (-1.0 to 1.0).
    pub directional_skew: f64,
    /// Per-symbol enabled status.
    pub symbol_enabled: HashMap<String, bool>,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            spread_multiplier: 1.0,
            size_scalar: 1.0,
            directional_skew: 0.0,
            symbol_enabled: HashMap::new(),
        }
    }
}

/// Event types broadcast by the market maker engine.
#[derive(Debug, Clone)]
pub enum MarketMakerEvent {
    /// Quote updated for an option.
    QuoteUpdated {
        /// Underlying symbol.
        symbol: String,
        /// Expiration date string.
        expiration: String,
        /// Strike price in cents.
        strike: u64,
        /// Option style (call/put).
        style: String,
        /// Bid price in cents.
        bid_price: u128,
        /// Ask price in cents.
        ask_price: u128,
        /// Bid size.
        bid_size: u64,
        /// Ask size.
        ask_size: u64,
    },
    /// Order filled.
    OrderFilled {
        /// Order identifier.
        order_id: String,
        /// Underlying symbol.
        symbol: String,
        /// Instrument identifier.
        instrument: String,
        /// Order side (buy/sell).
        side: String,
        /// Filled quantity.
        quantity: u64,
        /// Fill price in cents.
        price: u128,
        /// Edge captured in cents.
        edge: i64,
    },
    /// Configuration changed.
    ConfigChanged {
        /// Whether quoting is enabled.
        enabled: bool,
        /// Spread multiplier.
        spread_multiplier: f64,
        /// Size scalar (0.0 to 1.0).
        size_scalar: f64,
        /// Directional skew (-1.0 to 1.0).
        directional_skew: f64,
    },
    /// Underlying price updated.
    PriceUpdated {
        /// Underlying symbol.
        symbol: String,
        /// Price in cents.
        price_cents: u64,
    },
}

/// Active order information: (symbol, expiration, strike, style).
type ActiveOrderInfo = (String, String, u64, OptionStyle);

/// The market maker engine coordinates all quoting activity.
pub struct MarketMakerEngine {
    /// Order book manager.
    manager: Arc<UnderlyingOrderBookManager>,
    /// Database pool for persistence (reserved for future use).
    #[allow(dead_code)]
    db: Option<DatabasePool>,
    /// Option pricer (reserved for future use).
    #[allow(dead_code)]
    pricer: OptionPricer,
    /// Quoter for generating quotes.
    quoter: Quoter,
    /// Current configuration.
    config: Arc<RwLock<MarketMakerConfig>>,
    /// Latest underlying prices (symbol -> price in cents).
    prices: Arc<RwLock<HashMap<String, u64>>>,
    /// Active orders (order_id -> order info).
    active_orders: Arc<RwLock<HashMap<OrderId, ActiveOrderInfo>>>,
    /// Event broadcaster.
    event_tx: broadcast::Sender<MarketMakerEvent>,
}

impl MarketMakerEngine {
    /// Creates a new market maker engine.
    ///
    /// # Arguments
    /// * `manager` - Order book manager
    /// * `db` - Optional database pool
    #[must_use]
    pub fn new(manager: Arc<UnderlyingOrderBookManager>, db: Option<DatabasePool>) -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            manager,
            db,
            pricer: OptionPricer::default(),
            quoter: Quoter::default(),
            config: Arc::new(RwLock::new(MarketMakerConfig::default())),
            prices: Arc::new(RwLock::new(HashMap::new())),
            active_orders: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Returns a receiver for market maker events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<MarketMakerEvent> {
        self.event_tx.subscribe()
    }

    /// Updates the underlying price.
    ///
    /// # Arguments
    /// * `symbol` - Underlying symbol
    /// * `price_cents` - Price in cents
    pub fn update_price(&self, symbol: &str, price_cents: u64) {
        {
            let mut prices = self.prices.write();
            prices.insert(symbol.to_string(), price_cents);
        }

        let _ = self.event_tx.send(MarketMakerEvent::PriceUpdated {
            symbol: symbol.to_string(),
            price_cents,
        });

        debug!("Updated price for {}: {} cents", symbol, price_cents);

        // Trigger requote if enabled
        if self.is_enabled() && self.is_symbol_enabled(symbol) {
            self.requote_symbol(symbol);
        }
    }

    /// Gets the current price for a symbol.
    #[must_use]
    pub fn get_price(&self, symbol: &str) -> Option<u64> {
        self.prices.read().get(symbol).copied()
    }

    /// Checks if the market maker is globally enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.read().enabled
    }

    /// Checks if a specific symbol is enabled.
    #[must_use]
    pub fn is_symbol_enabled(&self, symbol: &str) -> bool {
        let config = self.config.read();
        config.symbol_enabled.get(symbol).copied().unwrap_or(true)
    }

    /// Enables or disables the market maker globally.
    pub fn set_enabled(&self, enabled: bool) {
        {
            let mut config = self.config.write();
            config.enabled = enabled;
        }

        info!(
            "Market maker {}",
            if enabled { "enabled" } else { "disabled" }
        );

        if !enabled {
            self.cancel_all_orders();
        }

        self.broadcast_config_change();
    }

    /// Enables or disables quoting for a specific symbol.
    pub fn set_symbol_enabled(&self, symbol: &str, enabled: bool) {
        {
            let mut config = self.config.write();
            config.symbol_enabled.insert(symbol.to_string(), enabled);
        }

        info!(
            "Symbol {} {}",
            symbol,
            if enabled { "enabled" } else { "disabled" }
        );

        if !enabled {
            self.cancel_symbol_orders(symbol);
        }
    }

    /// Updates the spread multiplier.
    ///
    /// A finite value is clamped into `[SPREAD_MULTIPLIER_MIN,
    /// SPREAD_MULTIPLIER_MAX]`. A non-finite (`NaN` / infinite) input is a no-op
    /// with a `WARN`: `f64::clamp` returns `NaN` for a `NaN` input, so clamping
    /// must never be the only guard. Callers should validate at the API boundary
    /// via [`validate_control_value`]; this is defense in depth.
    pub fn set_spread_multiplier(&self, multiplier: f64) {
        if !multiplier.is_finite() {
            warn!(value = multiplier, "ignoring non-finite spread multiplier");
            return;
        }
        {
            let mut config = self.config.write();
            config.spread_multiplier =
                multiplier.clamp(SPREAD_MULTIPLIER_MIN, SPREAD_MULTIPLIER_MAX);
        }
        self.broadcast_config_change();
        self.requote_all();
    }

    /// Updates the size scalar.
    ///
    /// A finite value is clamped into `[SIZE_SCALAR_MIN, SIZE_SCALAR_MAX]`. A
    /// non-finite (`NaN` / infinite) input is a no-op with a `WARN` so a `NaN`
    /// can never be stored (clamping a `NaN` yields `NaN`).
    pub fn set_size_scalar(&self, scalar: f64) {
        if !scalar.is_finite() {
            warn!(value = scalar, "ignoring non-finite size scalar");
            return;
        }
        {
            let mut config = self.config.write();
            config.size_scalar = scalar.clamp(SIZE_SCALAR_MIN, SIZE_SCALAR_MAX);
        }
        self.broadcast_config_change();
        self.requote_all();
    }

    /// Updates the directional skew.
    ///
    /// A finite value is clamped into `[DIRECTIONAL_SKEW_MIN,
    /// DIRECTIONAL_SKEW_MAX]`. A non-finite (`NaN` / infinite) input is a no-op
    /// with a `WARN` so a `NaN` can never be stored (clamping a `NaN` yields
    /// `NaN`).
    pub fn set_directional_skew(&self, skew: f64) {
        if !skew.is_finite() {
            warn!(value = skew, "ignoring non-finite directional skew");
            return;
        }
        {
            let mut config = self.config.write();
            config.directional_skew = skew.clamp(DIRECTIONAL_SKEW_MIN, DIRECTIONAL_SKEW_MAX);
        }
        self.broadcast_config_change();
        self.requote_all();
    }

    /// Gets the current configuration.
    #[must_use]
    pub fn get_config(&self) -> MarketMakerConfig {
        self.config.read().clone()
    }

    /// Cancels all active orders.
    pub fn cancel_all_orders(&self) {
        let orders: Vec<_> = self.active_orders.read().keys().copied().collect();

        for order_id in orders {
            self.cancel_order(order_id);
        }

        info!("Cancelled all orders");
    }

    /// Cancels all orders for a specific symbol.
    pub fn cancel_symbol_orders(&self, symbol: &str) {
        let orders: Vec<_> = self
            .active_orders
            .read()
            .iter()
            .filter(|(_, (s, _, _, _))| s == symbol)
            .map(|(id, _)| *id)
            .collect();

        for order_id in orders {
            self.cancel_order(order_id);
        }

        info!("Cancelled all orders for {}", symbol);
    }

    /// Cancels a specific order.
    fn cancel_order(&self, order_id: OrderId) {
        let order_info = self.active_orders.write().remove(&order_id);

        if let Some((symbol, exp_str, strike, style)) = order_info
            && let Ok(underlying_book) = self.manager.get(&symbol)
        {
            // Parse expiration and cancel order
            if let Ok(expiration) = self.parse_expiration(&exp_str)
                && let Ok(exp_book) = underlying_book.get_expiration(&expiration)
                && let Ok(strike_book) = exp_book.get_strike(strike)
            {
                let option_book = strike_book.get(style);
                let _ = option_book.cancel_order(order_id);
            }
        }
    }

    /// Requotes all instruments for a symbol.
    fn requote_symbol(&self, symbol: &str) {
        let price_cents = match self.get_price(symbol) {
            Some(p) => p,
            None => {
                warn!("No price available for {}, skipping requote", symbol);
                return;
            }
        };

        let config = self.get_config();

        if let Ok(underlying_book) = self.manager.get(symbol) {
            for (expiration, exp_book) in underlying_book.expirations().iter() {
                for strike in exp_book.strike_prices() {
                    if let Ok(_strike_book) = exp_book.get_strike(strike) {
                        for style in [OptionStyle::Call, OptionStyle::Put] {
                            self.update_quote(
                                symbol,
                                &expiration.to_string(),
                                strike,
                                style,
                                price_cents,
                                &config,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Requotes all instruments.
    fn requote_all(&self) {
        for symbol in self.manager.underlying_symbols() {
            if self.is_symbol_enabled(&symbol) {
                self.requote_symbol(&symbol);
            }
        }
    }

    /// Updates quotes for a specific option.
    fn update_quote(
        &self,
        symbol: &str,
        exp_str: &str,
        strike: u64,
        style: OptionStyle,
        spot_cents: u64,
        config: &MarketMakerConfig,
    ) {
        let expiration = match self.parse_expiration(exp_str) {
            Ok(e) => e,
            Err(_) => return,
        };

        let input = QuoteInput {
            spot_cents,
            strike_cents: strike,
            expiration: &expiration,
            style,
            spread_multiplier: config.spread_multiplier,
            size_scalar: config.size_scalar,
            directional_skew: config.directional_skew,
            iv: None,
        };

        // Skip the instrument when the theoretical value is non-finite: the
        // quoter returns `None` rather than producing a poisoned bid/ask. The
        // requote loop continues with the next strike/style.
        let quote_params = match self.quoter.generate_quote(&input) {
            Some(params) => params,
            None => {
                warn!(
                    symbol = %symbol,
                    expiration = %exp_str,
                    strike,
                    style = ?style,
                    "skipping quote: non-finite theoretical value"
                );
                return;
            }
        };

        // Cancel existing orders and place new ones
        if let Ok(underlying_book) = self.manager.get(symbol)
            && let Ok(exp_book) = underlying_book.get_expiration(&expiration)
            && let Ok(strike_book) = exp_book.get_strike(strike)
        {
            let option_book = strike_book.get(style);

            // Replace, don't accumulate: cancel this instrument's previously
            // resting maker orders before placing the fresh quote. Without this
            // every requote tick would add a new bid/ask pair while the prior
            // pair kept resting, leaking stale quotes into the option book.
            //
            // Match the exact instrument (symbol, expiration, strike, style) so
            // requoting a BTC 50000 Call never cancels its Put or another strike.
            // Collect the stale ids under a short read lock, drop the lock, cancel
            // each on the option book (never holding the lock across the cancel),
            // then prune the tracking map under one short write lock.
            let stale_ids: Vec<OrderId> = {
                let orders = self.active_orders.read();
                let mut ids = Vec::with_capacity(2);
                for (id, (s, e, k, sty)) in orders.iter() {
                    if s == symbol && e == exp_str && *k == strike && *sty == style {
                        ids.push(*id);
                    }
                }
                ids
            };
            if !stale_ids.is_empty() {
                for &stale_id in &stale_ids {
                    // Ok(true) = cancelled, Ok(false) = already filled/gone; both
                    // mean the order should leave tracking. Delegate the actual
                    // cancel to the upstream book.
                    let _ = option_book.cancel_order(stale_id);
                }
                let mut orders = self.active_orders.write();
                for stale_id in &stale_ids {
                    orders.remove(stale_id);
                }
                debug!(
                    symbol = %symbol,
                    expiration = %exp_str,
                    strike,
                    style = ?style,
                    cancelled = stale_ids.len(),
                    "cancelled stale quote before requote"
                );
            }

            // Place bid order
            let bid_id = OrderId::new();
            if option_book
                .add_limit_order(
                    bid_id,
                    Side::Buy,
                    quote_params.bid_price,
                    quote_params.bid_size,
                )
                .is_ok()
            {
                self.active_orders.write().insert(
                    bid_id,
                    (symbol.to_string(), exp_str.to_string(), strike, style),
                );
            }

            // Place ask order
            let ask_id = OrderId::new();
            if option_book
                .add_limit_order(
                    ask_id,
                    Side::Sell,
                    quote_params.ask_price,
                    quote_params.ask_size,
                )
                .is_ok()
            {
                self.active_orders.write().insert(
                    ask_id,
                    (symbol.to_string(), exp_str.to_string(), strike, style),
                );
            }

            // Broadcast quote update
            let _ = self.event_tx.send(MarketMakerEvent::QuoteUpdated {
                symbol: symbol.to_string(),
                expiration: exp_str.to_string(),
                strike,
                style: match style {
                    OptionStyle::Call => "call".to_string(),
                    OptionStyle::Put => "put".to_string(),
                },
                bid_price: quote_params.bid_price,
                ask_price: quote_params.ask_price,
                bid_size: quote_params.bid_size,
                ask_size: quote_params.ask_size,
            });
        }
    }

    /// Broadcasts a configuration change event.
    fn broadcast_config_change(&self) {
        let config = self.config.read();
        let _ = self.event_tx.send(MarketMakerEvent::ConfigChanged {
            enabled: config.enabled,
            spread_multiplier: config.spread_multiplier,
            size_scalar: config.size_scalar,
            directional_skew: config.directional_skew,
        });
    }

    /// Parses an expiration string.
    fn parse_expiration(&self, exp_str: &str) -> Result<optionstratlib::ExpirationDate, ()> {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
        use optionstratlib::ExpirationDate;
        use optionstratlib::prelude::Positive;

        // Try parsing as a number of days first.
        if let Ok(days) = exp_str.parse::<i32>() {
            // An expiration must be a strictly positive number of days; a
            // non-positive or otherwise invalid value is logged and rejected
            // rather than panicking the quoting loop on bad stored data.
            if days <= 0 {
                warn!(expiration = %exp_str, "rejecting non-positive expiration days");
                return Err(());
            }
            let positive_days = Positive::new(days as f64).map_err(|_| {
                warn!(expiration = %exp_str, "rejecting invalid expiration value");
            })?;
            return Ok(ExpirationDate::Days(positive_days));
        }

        // Try parsing as YYYYMMDD format. `len()` is a byte length, so the
        // `is_ascii()` guard ensures every byte is a char boundary before
        // slicing — an 8-byte multibyte string (e.g. `"12345é7"`) returns
        // `Err(())` instead of panicking the requote loop on a non-boundary slice.
        if exp_str.len() == 8
            && exp_str.is_ascii()
            && let (Ok(year), Ok(month), Ok(day)) = (
                exp_str[0..4].parse::<i32>(),
                exp_str[4..6].parse::<u32>(),
                exp_str[6..8].parse::<u32>(),
            )
            && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            // 16:00:00 is a compile-time literal time-of-day that is always valid.
            let time = NaiveTime::from_hms_opt(16, 0, 0)
                .expect("16:00:00 is a valid time-of-day constant");
            let datetime = NaiveDateTime::new(date, time);
            let utc_datetime = Utc.from_utc_datetime(&datetime);
            return Ok(ExpirationDate::DateTime(utc_datetime));
        }

        // Try parsing the `ExpirationDate` `Display` form
        // (`%Y-%m-%d %H:%M:%S UTC`). The requote loop keys instruments by
        // `expiration.to_string()` and feeds that same string back here (and into
        // `cancel_order`); without this branch a `DateTime` expiration could never
        // round-trip, so `update_quote` would early-return and never place — or
        // later cancel — any maker order. Parsing as a naive UTC datetime keeps
        // the reparsed `ExpirationKey` identical to the originally stored one.
        if let Ok(naive) = NaiveDateTime::parse_from_str(exp_str, "%Y-%m-%d %H:%M:%S UTC") {
            let utc_datetime = Utc.from_utc_datetime(&naive);
            return Ok(ExpirationDate::DateTime(utc_datetime));
        }

        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> MarketMakerEngine {
        MarketMakerEngine::new(Arc::new(UnderlyingOrderBookManager::new()), None)
    }

    /// A far-future absolute expiration whose `Display` form round-trips through
    /// [`MarketMakerEngine::parse_expiration`] to the identical `ExpirationKey`,
    /// so the requote loop can place (and later cancel) orders against it.
    fn future_expiration() -> optionstratlib::ExpirationDate {
        use chrono::{TimeZone, Utc};
        let dt = Utc
            .with_ymd_and_hms(2035, 12, 31, 16, 0, 0)
            .single()
            .expect("valid fixture datetime");
        optionstratlib::ExpirationDate::DateTime(dt)
    }

    /// Collects the engine's tracked order ids for one exact instrument.
    fn instrument_ids(
        engine: &MarketMakerEngine,
        symbol: &str,
        strike: u64,
        style: OptionStyle,
    ) -> std::collections::HashSet<OrderId> {
        engine
            .active_orders
            .read()
            .iter()
            .filter(|(_, (s, _, k, sty))| s == symbol && *k == strike && *sty == style)
            .map(|(id, _)| *id)
            .collect()
    }

    #[test]
    fn test_parse_expiration_round_trips_datetime_display() {
        // The requote loop keys instruments by `expiration.to_string()` and feeds
        // that string back through `parse_expiration`. A `DateTime` expiration must
        // round-trip, otherwise `update_quote` early-returns and never quotes.
        let engine = test_engine();
        let exp = future_expiration();
        let reparsed = engine
            .parse_expiration(&exp.to_string())
            .expect("display form must round-trip");
        // Same calendar instant => same expiration key the manager indexes on.
        match (exp, reparsed) {
            (
                optionstratlib::ExpirationDate::DateTime(a),
                optionstratlib::ExpirationDate::DateTime(b),
            ) => assert_eq!(a, b),
            other => panic!("expected two DateTime expirations, got {other:?}"),
        }
    }

    #[test]
    fn test_requote_does_not_accumulate_orders() {
        // Acceptance criteria for issue #49: after N requotes for the same
        // instrument the resting order count stays ~2 per instrument, not N*2.
        let engine = test_engine();
        let expiration = future_expiration();
        let underlying = engine.manager.get_or_create("BTC");
        let exp_book = underlying.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(5_000_000); // $50,000 strike
        let call_book = strike_book.get(OptionStyle::Call);
        let put_book = strike_book.get(OptionStyle::Put);

        // First tick places a bid+ask for the call and the put.
        engine.update_price("BTC", 5_000_000);
        assert_eq!(
            call_book.active_order_count(),
            2,
            "first requote should place exactly bid+ask for the call"
        );
        assert_eq!(
            put_book.active_order_count(),
            2,
            "first requote should place exactly bid+ask for the put"
        );
        assert_eq!(
            engine.active_orders.read().len(),
            4,
            "two instruments * (bid+ask) = 4 tracked orders"
        );

        // Second tick must REPLACE the resting quote, not stack a new pair on top.
        engine.update_price("BTC", 5_050_000);
        assert_eq!(
            call_book.active_order_count(),
            2,
            "requote must replace, not accumulate (call)"
        );
        assert_eq!(
            put_book.active_order_count(),
            2,
            "requote must replace, not accumulate (put)"
        );
        assert_eq!(
            engine.active_orders.read().len(),
            4,
            "tracking map must not grow per tick"
        );

        // Several more ticks: counts stay bounded, proving no per-tick leak.
        for px in [4_900_000, 5_100_000, 5_025_000, 4_950_000] {
            engine.update_price("BTC", px);
        }
        assert_eq!(call_book.active_order_count(), 2);
        assert_eq!(put_book.active_order_count(), 2);
        assert_eq!(engine.active_orders.read().len(), 4);
    }

    #[test]
    fn test_requote_one_instrument_leaves_others_resting() {
        // Per-instrument matching: requoting one contract must cancel only that
        // contract's stale orders, never another strike's or the same strike's
        // opposite style.
        let engine = test_engine();
        let expiration = future_expiration();
        let exp_str = expiration.to_string();
        let config = engine.get_config();

        let underlying = engine.manager.get_or_create("ETH");
        let exp_book = underlying.get_or_create_expiration(expiration);
        let strike_a = exp_book.get_or_create_strike(300_000); // $3,000
        let strike_b = exp_book.get_or_create_strike(400_000); // $4,000
        let a_call = strike_a.get(OptionStyle::Call);
        let a_put = strike_a.get(OptionStyle::Put);
        let b_call = strike_b.get(OptionStyle::Call);

        // Quote strike-A call, strike-A put, and strike-B call.
        engine.update_quote(
            "ETH",
            &exp_str,
            300_000,
            OptionStyle::Call,
            350_000,
            &config,
        );
        engine.update_quote("ETH", &exp_str, 300_000, OptionStyle::Put, 350_000, &config);
        engine.update_quote(
            "ETH",
            &exp_str,
            400_000,
            OptionStyle::Call,
            350_000,
            &config,
        );
        assert_eq!(a_call.active_order_count(), 2);
        assert_eq!(a_put.active_order_count(), 2);
        assert_eq!(b_call.active_order_count(), 2);

        let a_put_before = instrument_ids(&engine, "ETH", 300_000, OptionStyle::Put);
        let b_call_before = instrument_ids(&engine, "ETH", 400_000, OptionStyle::Call);
        assert_eq!(a_put_before.len(), 2);
        assert_eq!(b_call_before.len(), 2);

        // Requote ONLY the strike-A call.
        engine.update_quote(
            "ETH",
            &exp_str,
            300_000,
            OptionStyle::Call,
            351_000,
            &config,
        );

        // Strike-A call replaced (still 2); the others are untouched, same ids.
        assert_eq!(a_call.active_order_count(), 2, "strike-A call replaced");
        assert_eq!(a_put.active_order_count(), 2, "strike-A put untouched");
        assert_eq!(b_call.active_order_count(), 2, "strike-B call untouched");
        assert_eq!(
            instrument_ids(&engine, "ETH", 300_000, OptionStyle::Put),
            a_put_before,
            "requoting the call must not disturb the same-strike put"
        );
        assert_eq!(
            instrument_ids(&engine, "ETH", 400_000, OptionStyle::Call),
            b_call_before,
            "requoting strike A must not disturb strike B"
        );
    }

    #[test]
    fn test_parse_expiration_rejects_zero() {
        let engine = test_engine();
        assert!(engine.parse_expiration("0").is_err());
    }

    #[test]
    fn test_parse_expiration_rejects_negative() {
        let engine = test_engine();
        assert!(engine.parse_expiration("-5").is_err());
    }

    #[test]
    fn test_parse_expiration_rejects_garbage() {
        let engine = test_engine();
        assert!(engine.parse_expiration("not-a-date").is_err());
    }

    #[test]
    fn test_parse_expiration_accepts_positive_days() {
        let engine = test_engine();
        assert!(engine.parse_expiration("30").is_ok());
    }

    #[test]
    fn test_parse_expiration_accepts_yyyymmdd() {
        let engine = test_engine();
        assert!(engine.parse_expiration("20251231").is_ok());
    }

    #[test]
    fn test_parse_expiration_rejects_multibyte_eight_bytes() {
        // `"12345é7"` is 8 bytes ('é' is 2 bytes) but does not parse as i32, so it
        // reaches the YYYYMMDD branch where byte slicing at indices 4/6 would land
        // mid-char and panic the requote loop. The char-safe guard must return
        // `Err(())` instead.
        let engine = test_engine();
        let multibyte = "12345é7";
        assert_eq!(multibyte.len(), 8, "fixture must be exactly 8 bytes");
        assert!(engine.parse_expiration(multibyte).is_err());
    }

    // ------------------------------------------------------------------------
    // validate_control_value
    // ------------------------------------------------------------------------

    #[test]
    fn test_validate_control_value_accepts_in_range() {
        assert_eq!(
            validate_control_value("spread_multiplier", 2.0, 0.1, 10.0),
            Ok(2.0)
        );
        // Inclusive bounds.
        assert_eq!(
            validate_control_value("size_scalar", 0.0, 0.0, 1.0),
            Ok(0.0)
        );
        assert_eq!(
            validate_control_value("size_scalar", 1.0, 0.0, 1.0),
            Ok(1.0)
        );
        assert_eq!(
            validate_control_value("directional_skew", -1.0, -1.0, 1.0),
            Ok(-1.0)
        );
    }

    #[test]
    fn test_validate_control_value_rejects_non_finite() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = validate_control_value("spread_multiplier", bad, 0.1, 10.0)
                .expect_err("non-finite must be rejected");
            assert!(err.contains("spread_multiplier"));
            assert!(err.contains("must be finite and within"));
        }
    }

    #[test]
    fn test_validate_control_value_rejects_out_of_range() {
        assert!(validate_control_value("spread_multiplier", 0.05, 0.1, 10.0).is_err());
        assert!(validate_control_value("spread_multiplier", 10.5, 0.1, 10.0).is_err());
        assert!(validate_control_value("directional_skew", -1.5, -1.0, 1.0).is_err());
        assert!(validate_control_value("directional_skew", 1.5, -1.0, 1.0).is_err());
    }

    // ------------------------------------------------------------------------
    // Engine setters: non-finite is a no-op (defense in depth)
    // ------------------------------------------------------------------------

    #[test]
    fn test_set_spread_multiplier_nan_is_noop() {
        let engine = test_engine();
        let before = engine.get_config().spread_multiplier;
        engine.set_spread_multiplier(f64::NAN);
        let after = engine.get_config().spread_multiplier;
        assert_eq!(before, after);
        assert!(after.is_finite());
    }

    #[test]
    fn test_set_size_scalar_inf_is_noop() {
        let engine = test_engine();
        let before = engine.get_config().size_scalar;
        engine.set_size_scalar(f64::INFINITY);
        let after = engine.get_config().size_scalar;
        assert_eq!(before, after);
        assert!(after.is_finite());
    }

    #[test]
    fn test_set_directional_skew_neg_inf_is_noop() {
        let engine = test_engine();
        let before = engine.get_config().directional_skew;
        engine.set_directional_skew(f64::NEG_INFINITY);
        let after = engine.get_config().directional_skew;
        assert_eq!(before, after);
        assert!(after.is_finite());
    }

    #[test]
    fn test_set_spread_multiplier_finite_is_clamped() {
        let engine = test_engine();
        engine.set_spread_multiplier(100.0);
        assert_eq!(engine.get_config().spread_multiplier, SPREAD_MULTIPLIER_MAX);
        engine.set_spread_multiplier(0.0);
        assert_eq!(engine.get_config().spread_multiplier, SPREAD_MULTIPLIER_MIN);
    }
}

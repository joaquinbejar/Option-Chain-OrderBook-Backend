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
        /// Edge captured in cents PER CONTRACT, against the quote-time
        /// theoretical value (total capture = `edge × quantity`).
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

/// A market-maker order resting on a book, tracked for cancel-on-requote and
/// fill detection (issue #69).
#[derive(Debug, Clone)]
struct ActiveOrderInfo {
    /// Underlying symbol.
    symbol: String,
    /// Expiration string (the book key, as used for book lookup on cancel).
    expiration: String,
    /// Canonical `UNDERLYING-YYYYMMDD-STRIKE-STYLE` identifier as used across
    /// the REST surface, carried on fill events.
    instrument: String,
    /// Strike price in cents.
    strike: u64,
    /// Call or Put.
    style: OptionStyle,
    /// True for the bid leg, false for the ask leg.
    is_buy: bool,
    /// Theoretical value in cents at quote time, for edge computation.
    theo_cents: u64,
    /// Remaining resting quantity.
    quantity: u64,
}

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
            .filter(|(_, order)| order.symbol == symbol)
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

        if let Some(order) = order_info
            && let Ok(underlying_book) = self.manager.get(&order.symbol)
        {
            // Parse expiration and cancel order
            if let Ok(expiration) = self.parse_expiration(&order.expiration)
                && let Ok(exp_book) = underlying_book.get_expiration(&expiration)
                && let Ok(strike_book) = exp_book.get_strike(order.strike)
            {
                let option_book = strike_book.get(order.style);
                let _ = option_book.cancel_order(order_id);
            }
        }
    }

    /// Notifies the engine that one of its resting quotes was (partially)
    /// filled (issue #69).
    ///
    /// If `order_id` is a tracked market-maker order, the captured edge is
    /// computed against the quote-time theoretical value via
    /// [`Quoter::calculate_edge`], the tracked quantity is reduced (the order
    /// is removed once fully filled), and a
    /// [`MarketMakerEvent::OrderFilled`] is broadcast. Order ids that do not
    /// belong to the market maker are ignored, so callers can report every
    /// fill's maker id without pre-filtering.
    pub fn on_order_filled(&self, order_id: OrderId, fill_price_cents: u128, quantity: u64) {
        if quantity == 0 {
            return;
        }

        // Cheap read-gate: most fills are user-vs-user, so never take the
        // write lock for ids the engine does not own. (Benign TOCTOU: an
        // order removed between this check and the write below is skipped.)
        if !self.active_orders.read().contains_key(&order_id) {
            return;
        }

        // Update under one short write lock; no lock held across the
        // broadcast below. A full fill moves the removed value out (no
        // clone); a partial fill clones and keeps the decremented remainder.
        // The reported quantity is clamped to the tracked resting size so an
        // upstream over-fill can never inflate the event.
        let (order, reported_qty) = {
            let mut orders = self.active_orders.write();
            let Some(remaining) = orders.get(&order_id).map(|o| o.quantity) else {
                return;
            };
            let reported = quantity.min(remaining);
            if quantity >= remaining {
                match orders.remove(&order_id) {
                    Some(order) => (order, reported),
                    None => return,
                }
            } else {
                match orders.get_mut(&order_id) {
                    Some(order) => {
                        // Guarded: quantity < remaining here.
                        order.quantity -= quantity;
                        (order.clone(), reported)
                    }
                    None => return,
                }
            }
        };

        // The market-data DTOs carry prices as u64 cents; a fill price beyond
        // that range is structurally impossible — log and skip rather than
        // truncating money.
        let fill_u64 = match u64::try_from(fill_price_cents) {
            Ok(p) => p,
            Err(_) => {
                warn!(
                    symbol = %order.symbol,
                    price = fill_price_cents,
                    "market-maker fill price exceeds u64 cents range; skipping fill event"
                );
                return;
            }
        };

        let edge = Quoter::calculate_edge(fill_u64, order.theo_cents, order.is_buy);
        let side = if order.is_buy { "buy" } else { "sell" };

        let _ = self.event_tx.send(MarketMakerEvent::OrderFilled {
            order_id: order_id.to_string(),
            symbol: order.symbol,
            instrument: order.instrument,
            side: side.to_string(),
            quantity: reported_qty,
            price: fill_price_cents,
            edge,
        });
    }

    /// Test-only: registers a tracked market-maker order directly (bypassing
    /// the book) so the `record_fills` → `on_order_filled` seam can be
    /// exercised without a matching engine.
    #[cfg(test)]
    pub(crate) fn track_order_for_test(
        &self,
        is_buy: bool,
        theo_cents: u64,
        quantity: u64,
    ) -> OrderId {
        let id = OrderId::new();
        self.active_orders.write().insert(
            id,
            ActiveOrderInfo {
                symbol: "BTC".to_string(),
                expiration: "20351231".to_string(),
                instrument: "BTC-20351231-100000-C".to_string(),
                strike: 100_000,
                style: OptionStyle::Call,
                is_buy,
                theo_cents,
                quantity,
            },
        );
        id
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

        // Canonical instrument identifier (UNDERLYING-YYYYMMDD-STRIKE-STYLE)
        // as used across the REST surface, carried on fill events (issue #69).
        // Falls back to the raw book key when the expiration carries no
        // calendar date.
        let exp_canonical = expiration
            .get_date()
            .map(|d| d.format("%Y%m%d").to_string())
            .unwrap_or_else(|_| exp_str.to_string());
        let style_char = match style {
            OptionStyle::Call => "C",
            OptionStyle::Put => "P",
        };
        let instrument = format!("{symbol}-{exp_canonical}-{strike}-{style_char}");

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
                for (id, order) in orders.iter() {
                    if order.symbol == symbol
                        && order.expiration == exp_str
                        && order.strike == strike
                        && order.style == style
                    {
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
                    ActiveOrderInfo {
                        symbol: symbol.to_string(),
                        expiration: exp_str.to_string(),
                        instrument: instrument.clone(),
                        strike,
                        style,
                        is_buy: true,
                        theo_cents: quote_params.theo_price,
                        quantity: quote_params.bid_size,
                    },
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
                    ActiveOrderInfo {
                        symbol: symbol.to_string(),
                        expiration: exp_str.to_string(),
                        instrument: instrument.clone(),
                        strike,
                        style,
                        is_buy: false,
                        theo_cents: quote_params.theo_price,
                        quantity: quote_params.ask_size,
                    },
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

        // An 8-ASCII-digit string is ALWAYS a YYYYMMDD calendar date — this
        // branch must run BEFORE the numeric-days branch (issue #110) so the
        // engine resolves the same expiration key as the API handlers. Every
        // byte being an ASCII digit also makes each byte a char boundary, so
        // the slicing below cannot panic.
        if exp_str.len() == 8 && exp_str.bytes().all(|b| b.is_ascii_digit()) {
            if let (Ok(year), Ok(month), Ok(day)) = (
                exp_str[0..4].parse::<i32>(),
                exp_str[4..6].parse::<u32>(),
                exp_str[6..8].parse::<u32>(),
            ) && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
                && let Some(time) = NaiveTime::from_hms_opt(16, 0, 0)
            {
                let datetime = NaiveDateTime::new(date, time);
                let utc_datetime = Utc.from_utc_datetime(&datetime);
                return Ok(ExpirationDate::DateTime(utc_datetime));
            }
            // 8 digits that are not a real calendar date: reject, never a
            // relative-days fallback.
            warn!(expiration = %exp_str, "rejecting invalid 8-digit expiration date");
            return Err(());
        }

        // Any other numeric string is a relative number of days.
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
            .filter(|(_, order)| {
                order.symbol == symbol && order.strike == strike && order.style == style
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Inserts a tracked market-maker order directly (bypassing the book) so
    /// fill-notification behavior can be tested in isolation.
    fn track_order(engine: &MarketMakerEngine, is_buy: bool, theo_cents: u64, qty: u64) -> OrderId {
        engine.track_order_for_test(is_buy, theo_cents, qty)
    }

    /// Issue #69: a fill on a tracked maker order broadcasts OrderFilled with
    /// the edge computed against the quote-time theo, and a full fill removes
    /// the order from tracking.
    #[test]
    fn test_on_order_filled_broadcasts_edge_and_untracks() {
        let engine = test_engine();
        let mut events = engine.subscribe();

        // A bid (buy) filled at 95 with theo 100 captures +5 edge.
        let id = track_order(&engine, true, 100, 10);
        engine.on_order_filled(id, 95, 10);

        let event = events.try_recv().expect("OrderFilled must be broadcast");
        match event {
            MarketMakerEvent::OrderFilled {
                order_id,
                symbol,
                instrument,
                side,
                quantity,
                price,
                edge,
            } => {
                assert_eq!(order_id, id.to_string());
                assert_eq!(symbol, "BTC");
                assert_eq!(instrument, "BTC-20351231-100000-C");
                assert_eq!(side, "buy");
                assert_eq!(quantity, 10);
                assert_eq!(price, 95);
                assert_eq!(edge, 5, "buy edge = theo - fill");
            }
            other => panic!("expected OrderFilled, got {other:?}"),
        }

        // Fully filled -> no longer tracked.
        assert!(!engine.active_orders.read().contains_key(&id));
    }

    /// A sell fill above theo captures positive edge; a partial fill keeps the
    /// order tracked with the remaining quantity.
    #[test]
    fn test_on_order_filled_partial_sell_keeps_remainder() {
        let engine = test_engine();
        let mut events = engine.subscribe();

        // An ask (sell) filled at 110 with theo 105 captures +5 edge.
        let id = track_order(&engine, false, 105, 10);
        engine.on_order_filled(id, 110, 4);

        match events.try_recv().expect("OrderFilled must be broadcast") {
            MarketMakerEvent::OrderFilled {
                side,
                quantity,
                edge,
                ..
            } => {
                assert_eq!(side, "sell");
                assert_eq!(quantity, 4);
                assert_eq!(edge, 5, "sell edge = fill - theo");
            }
            other => panic!("expected OrderFilled, got {other:?}"),
        }

        // Partially filled -> still tracked with the remainder.
        let orders = engine.active_orders.read();
        let order = orders.get(&id).expect("partially filled order stays");
        assert_eq!(order.quantity, 6);
    }

    /// Fills on unknown (non-market-maker) order ids are ignored: no event,
    /// no tracking change.
    #[test]
    fn test_on_order_filled_ignores_unknown_orders() {
        let engine = test_engine();
        let mut events = engine.subscribe();

        engine.on_order_filled(OrderId::new(), 100, 1);

        assert!(
            events.try_recv().is_err(),
            "no event may be broadcast for a non-market-maker fill"
        );
    }

    /// A zero-quantity notification is ignored, and an upstream over-fill can
    /// never inflate the event: the reported quantity is clamped to the
    /// tracked resting size.
    #[test]
    fn test_on_order_filled_guards_zero_and_overfill() {
        let engine = test_engine();
        let mut events = engine.subscribe();

        let id = track_order(&engine, true, 100, 10);
        engine.on_order_filled(id, 95, 0);
        assert!(
            events.try_recv().is_err(),
            "a zero-quantity fill must not broadcast"
        );

        engine.on_order_filled(id, 95, 15);
        match events.try_recv().expect("over-fill still broadcasts once") {
            MarketMakerEvent::OrderFilled { quantity, .. } => {
                assert_eq!(quantity, 10, "reported quantity clamps to the resting size");
            }
            other => panic!("expected OrderFilled, got {other:?}"),
        }
        assert!(!engine.active_orders.read().contains_key(&id));
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

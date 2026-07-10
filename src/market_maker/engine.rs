//! Market maker engine that coordinates quoting across all instruments.

use crate::db::DatabasePool;
use crate::market_maker::{OptionPricer, QuoteInput, Quoter};
use chrono::{DateTime, Utc};
use option_chain_orderbook::orderbook::UnderlyingOrderBookManager;
use optionstratlib::prelude::Positive;
use optionstratlib::{ExpirationDate, OptionStyle};
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
    /// Structural expiration (the original book key), used for book lookup on
    /// cancel and as the source for the reverse-index key.
    ///
    /// Stored as the [`ExpirationDate`] itself (which is `Copy`) rather than its
    /// `Display` string: the `Days` variant's `Display` resolves `Utc::now() +
    /// n days` to the second, so a string key would drift between requote ticks
    /// and would not even round-trip back to the correct book key (`Days` vs
    /// `DateTime`). Keeping the structural value makes cancel and stale-lookup
    /// clock-independent (issue #107 P2-03).
    expiration: ExpirationDate,
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

/// Structural, clock-independent projection of an [`ExpirationDate`] suitable as
/// a hash-map key.
///
/// `ExpirationDate`'s own `Eq`/`Hash` are wall-clock-relative (comparisons route
/// through `get_days()` against `Utc::now()`), and its `Display` for the `Days`
/// variant resolves `Utc::now() + n days` to the second — so neither can key a
/// resting order across requote ticks without drifting (issue #107 P2-03).
/// `ExpKey` mirrors the two `ExpirationDate` representations structurally: the
/// relative day count (`Positive`, `NaN`-free by construction) or the absolute
/// UTC instant. Both are `Eq + Hash` and never touch the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ExpKey {
    /// Relative expiration measured in fractional days.
    Days(Positive),
    /// Absolute expiration instant in UTC.
    DateTime(DateTime<Utc>),
}

impl From<&ExpirationDate> for ExpKey {
    /// Builds the structural, clock-independent key for an [`ExpirationDate`].
    #[inline]
    fn from(expiration: &ExpirationDate) -> Self {
        match expiration {
            ExpirationDate::Days(days) => Self::Days(*days),
            ExpirationDate::DateTime(dt) => Self::DateTime(*dt),
        }
    }
}

/// Structural identity of a quotable instrument, used as the key of the reverse
/// index that turns stale-order lookup on requote from an O(total_orders) scan
/// into an O(1) probe (issue #107 P2-01).
///
/// Clock-independent by construction (see [`ExpKey`]), so a `Days`-variant
/// expiration keys the same on every tick.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct InstrumentKey {
    /// Underlying symbol.
    symbol: String,
    /// Structural expiration key.
    expiration: ExpKey,
    /// Strike price in cents.
    strike: u64,
    /// Call or Put.
    style: OptionStyle,
}

impl InstrumentKey {
    /// Builds the structural key for an instrument leg.
    #[inline]
    fn new(symbol: &str, expiration: &ExpirationDate, strike: u64, style: OptionStyle) -> Self {
        Self {
            symbol: symbol.to_string(),
            expiration: ExpKey::from(expiration),
            strike,
            style,
        }
    }

    /// Rebuilds the key from a tracked order's metadata, for index cleanup on
    /// cancel and fill.
    #[inline]
    fn from_order(order: &ActiveOrderInfo) -> Self {
        Self {
            symbol: order.symbol.clone(),
            expiration: ExpKey::from(&order.expiration),
            strike: order.strike,
            style: order.style,
        }
    }
}

/// Loop-invariant context for [`MarketMakerEngine::update_quote`], built once per
/// expiration in `requote_symbol` and shared across every strike/style so the
/// hot inner loop borrows rather than re-derives it.
struct RequoteContext<'a> {
    /// Underlying symbol.
    symbol: &'a str,
    /// Structural book key (clock-independent), used for the book lookup and the
    /// reverse-index key.
    expiration: &'a ExpirationDate,
    /// Pre-built `Display` string of `expiration`, needed only for the broadcast
    /// event (hoisted once per expiration; issue #107 P2-02).
    exp_display: &'a str,
    /// Pre-built canonical `%Y%m%d` segment of `expiration` for the instrument
    /// identifier (hoisted once per expiration; falls back to `exp_display`
    /// when the expiration carries no calendar date).
    exp_canonical: &'a str,
    /// Current underlying price in cents.
    spot_cents: u64,
    /// Market-maker configuration snapshot for this requote pass.
    config: &'a MarketMakerConfig,
}

/// Maps `is_buy` to its reverse-index slot: the bid leg occupies slot 0, the ask
/// leg slot 1.
#[inline]
const fn leg_slot(is_buy: bool) -> usize {
    // is_buy == true  -> bid -> 0
    // is_buy == false -> ask -> 1
    !is_buy as usize
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
    /// Active orders (order_id -> order info). The source of truth for order
    /// metadata.
    active_orders: Arc<RwLock<HashMap<OrderId, ActiveOrderInfo>>>,
    /// Reverse index from an instrument's structural identity to its ≤2 resting
    /// maker order ids — slot 0 the bid leg, slot 1 the ask leg. A pure lookup
    /// accelerator over `active_orders`: it turns the per-requote stale-order
    /// lookup from an O(total_orders) scan into an O(1) probe (issue #107 P2-01).
    ///
    /// Invariant: every tracked order's leg is recorded here, and every id
    /// recorded here is present in `active_orders` — or is a transiently
    /// dangling id that a concurrent full fill removed between the two
    /// (sequential, never-nested) updates; such an id is benign because
    /// `OrderId`s are globally unique (its cancel matches nothing) and the
    /// next requote discards it. Both maps are updated together on insert /
    /// cancel / fill / eviction.
    ///
    /// Lock ordering: `active_orders` and `instrument_orders` are always locked
    /// **sequentially, never nested** — a critical section releases one before
    /// acquiring the other, and neither guard is ever held across a book call or
    /// a broadcast send. Should a future change ever need both at once, acquire
    /// `active_orders` before `instrument_orders`.
    instrument_orders: Arc<RwLock<HashMap<InstrumentKey, [Option<OrderId>; 2]>>>,
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
            instrument_orders: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Clears one leg (`is_buy` selects the bid or ask slot) from the
    /// reverse-index entry for `key`, dropping the whole entry once both legs
    /// are gone. Acquires only the `instrument_orders` lock, briefly.
    fn clear_instrument_slot(&self, key: &InstrumentKey, is_buy: bool) {
        let mut index = self.instrument_orders.write();
        if let Some(slots) = index.get_mut(key) {
            slots[leg_slot(is_buy)] = None;
            if slots[0].is_none() && slots[1].is_none() {
                index.remove(key);
            }
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

        // Every `cancel_order` already cleared its own reverse-index slot, but
        // clear the whole index so the kill switch leaves no dangling entry.
        self.instrument_orders.write().clear();

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

        if let Some(order) = order_info {
            // Drop this leg from the reverse index (sequential with the
            // `active_orders` lock above — the two maps are never held at once).
            self.clear_instrument_slot(&InstrumentKey::from_order(&order), order.is_buy);

            // Cancel on the book using the stored structural expiration (no
            // string reparse), so the lookup keys the same book that placed the
            // order for both `DateTime` and `Days` variants (issue #107).
            if let Ok(underlying_book) = self.manager.get(&order.symbol)
                && let Ok(exp_book) = underlying_book.get_expiration(&order.expiration)
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
        let (order, reported_qty, fully_filled) = {
            let mut orders = self.active_orders.write();
            let Some(remaining) = orders.get(&order_id).map(|o| o.quantity) else {
                return;
            };
            let reported = quantity.min(remaining);
            if quantity >= remaining {
                match orders.remove(&order_id) {
                    Some(order) => (order, reported, true),
                    None => return,
                }
            } else {
                match orders.get_mut(&order_id) {
                    Some(order) => {
                        // Guarded: quantity < remaining here.
                        order.quantity -= quantity;
                        (order.clone(), reported, false)
                    }
                    None => return,
                }
            }
        };

        // A fully-filled leg leaves the book, so drop it from the reverse index
        // too; a partial fill keeps the order resting, so its slot stays. Done
        // after releasing the `active_orders` lock — the two maps are never held
        // at once.
        if fully_filled {
            self.clear_instrument_slot(&InstrumentKey::from_order(&order), order.is_buy);
        }

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
        use chrono::TimeZone;
        let id = OrderId::new();
        let expiration = ExpirationDate::DateTime(
            Utc.with_ymd_and_hms(2035, 12, 31, 16, 0, 0)
                .single()
                .expect("valid fixture datetime"),
        );
        let info = ActiveOrderInfo {
            symbol: "BTC".to_string(),
            expiration,
            instrument: "BTC-20351231-100000-C".to_string(),
            strike: 100_000,
            style: OptionStyle::Call,
            is_buy,
            theo_cents,
            quantity,
        };
        let key = InstrumentKey::from_order(&info);
        self.active_orders.write().insert(id, info);
        self.instrument_orders
            .write()
            .entry(key)
            .or_insert([None, None])[leg_slot(is_buy)] = Some(id);
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
                // Hoisted out of the strike/style loops: the `Display` string is
                // only needed for the broadcast event, so build it once per
                // expiration instead of re-formatting per strike*style
                // (issue #107 P2-02). The book lookup and the reverse-index key
                // use the structural `ExpirationDate` directly, never this string.
                let exp_display = expiration.to_string();
                let exp_canonical = expiration
                    .get_date()
                    .map(|d| d.format("%Y%m%d").to_string())
                    .unwrap_or_else(|_| exp_display.clone());
                let ctx = RequoteContext {
                    symbol,
                    expiration: &expiration,
                    exp_display: &exp_display,
                    exp_canonical: &exp_canonical,
                    spot_cents: price_cents,
                    config: &config,
                };
                for strike in exp_book.strike_prices() {
                    if exp_book.get_strike(strike).is_ok() {
                        for style in [OptionStyle::Call, OptionStyle::Put] {
                            self.update_quote(&ctx, strike, style);
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

    /// Updates quotes for a specific option `strike`/`style`.
    ///
    /// The loop-invariant inputs (symbol, structural expiration, its pre-built
    /// `Display` string, spot, config) are carried in `ctx` so the hot inner
    /// loop borrows them. `ctx.expiration` is the structural book key (used for
    /// the book lookup and the reverse-index key, both clock-independent);
    /// `ctx.exp_display` is needed only for the broadcast event (issue #107).
    fn update_quote(&self, ctx: &RequoteContext<'_>, strike: u64, style: OptionStyle) {
        let symbol = ctx.symbol;
        let expiration = ctx.expiration;

        // Canonical instrument identifier (UNDERLYING-YYYYMMDD-STRIKE-STYLE)
        // as used across the REST surface, carried on fill events (issue #69);
        // the date segment is pre-built once per expiration in the context.
        let exp_canonical = ctx.exp_canonical;
        let style_char = match style {
            OptionStyle::Call => "C",
            OptionStyle::Put => "P",
        };
        let instrument = format!("{symbol}-{exp_canonical}-{strike}-{style_char}");

        let input = QuoteInput {
            spot_cents: ctx.spot_cents,
            strike_cents: strike,
            expiration,
            style,
            spread_multiplier: ctx.config.spread_multiplier,
            size_scalar: ctx.config.size_scalar,
            directional_skew: ctx.config.directional_skew,
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
                    expiration = %ctx.exp_display,
                    strike,
                    style = ?style,
                    "skipping quote: non-finite theoretical value"
                );
                return;
            }
        };

        // Structural, clock-independent identity of this exact instrument, used
        // for the O(1) reverse-index lookup below (issue #107 P2-01).
        let instrument_key = InstrumentKey::new(symbol, expiration, strike, style);

        // Cancel existing orders and place new ones.
        if let Ok(underlying_book) = self.manager.get(symbol)
            && let Ok(exp_book) = underlying_book.get_expiration(expiration)
            && let Ok(strike_book) = exp_book.get_strike(strike)
        {
            let option_book = strike_book.get(style);

            // Replace, don't accumulate: look up this exact instrument's ≤2
            // previously-resting maker orders in O(1) via the reverse index
            // (was an O(total_orders) scan over every tracked order; issue #107
            // P2-01). Copy the fixed-size slot array out under a short read lock,
            // drop the lock, cancel each id on the option book (never holding a
            // lock across the cancel), then prune both maps.
            let stale: [Option<OrderId>; 2] = self
                .instrument_orders
                .read()
                .get(&instrument_key)
                .copied()
                .unwrap_or_default();
            if stale.iter().any(Option::is_some) {
                for stale_id in stale.into_iter().flatten() {
                    // Ok(true) = cancelled, Ok(false) = already filled/gone; both
                    // mean the order should leave tracking. Delegate the actual
                    // cancel to the upstream book.
                    let _ = option_book.cancel_order(stale_id);
                }
                // Sequential locks (never nested): prune `active_orders`, release
                // it, then remove the reverse-index entry.
                {
                    let mut orders = self.active_orders.write();
                    for stale_id in stale.into_iter().flatten() {
                        orders.remove(&stale_id);
                    }
                }
                self.instrument_orders.write().remove(&instrument_key);
                debug!(
                    symbol = %symbol,
                    expiration = %ctx.exp_display,
                    strike,
                    style = ?style,
                    cancelled = stale.iter().filter(|s| s.is_some()).count(),
                    "cancelled stale quote before requote"
                );
            }

            // Place the fresh bid/ask, recording each leg's id for the reverse
            // index (slot 0 = bid, slot 1 = ask). The instrument string is built
            // once and moved into the ask leg (the bid clones it).
            let mut placed: [Option<OrderId>; 2] = [None, None];

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
                        expiration: *expiration,
                        instrument: instrument.clone(),
                        strike,
                        style,
                        is_buy: true,
                        theo_cents: quote_params.theo_price,
                        quantity: quote_params.bid_size,
                    },
                );
                placed[leg_slot(true)] = Some(bid_id);
            }

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
                        expiration: *expiration,
                        instrument,
                        strike,
                        style,
                        is_buy: false,
                        theo_cents: quote_params.theo_price,
                        quantity: quote_params.ask_size,
                    },
                );
                placed[leg_slot(false)] = Some(ask_id);
            }

            // Record both fresh legs in the reverse index under one write lock,
            // moving the key in (no clone) when at least one leg rested.
            if placed.iter().any(Option::is_some) {
                self.instrument_orders
                    .write()
                    .insert(instrument_key, placed);
            }

            // Broadcast quote update.
            let _ = self.event_tx.send(MarketMakerEvent::QuoteUpdated {
                symbol: symbol.to_string(),
                expiration: ctx.exp_display.to_string(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> MarketMakerEngine {
        MarketMakerEngine::new(Arc::new(UnderlyingOrderBookManager::new()), None)
    }

    /// A far-future absolute (`DateTime`) expiration the requote loop can place
    /// (and later cancel) orders against. Kept structurally by the engine, so the
    /// book lookup and reverse-index key are stable across ticks.
    fn future_expiration() -> optionstratlib::ExpirationDate {
        use chrono::{TimeZone, Utc};
        let dt = Utc
            .with_ymd_and_hms(2035, 12, 31, 16, 0, 0)
            .single()
            .expect("valid fixture datetime");
        optionstratlib::ExpirationDate::DateTime(dt)
    }

    /// A relative `Days` expiration whose `Display` form is clock-dependent
    /// (`Utc::now() + n days`, resolved to the second): the exact case issue
    /// #107 (P2-03) flags for stale-order key drift across requote ticks.
    fn days_expiration() -> optionstratlib::ExpirationDate {
        use optionstratlib::prelude::Positive;
        optionstratlib::ExpirationDate::Days(
            Positive::new(45.0).expect("45 is a valid positive day count"),
        )
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
    fn test_requote_days_expiration_does_not_accumulate_orders() {
        // Issue #107 (P2-03): a `Days`-variant expiration's `Display` string is
        // clock-dependent (`Utc::now() + 45d`, to the second), so keying stale
        // orders on that string drifts tick-to-tick and the requote loop would
        // either never place (book-key mismatch) or accumulate. Keying on the
        // structural expiration (clock-independent) must place exactly bid+ask
        // per instrument and keep that bounded across ticks.
        let engine = test_engine();
        let expiration = days_expiration();
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
            "a Days expiration must be quoted: bid+ask for the call"
        );
        assert_eq!(
            put_book.active_order_count(),
            2,
            "a Days expiration must be quoted: bid+ask for the put"
        );

        // Several more ticks: counts stay bounded, proving no per-tick leak even
        // though the Days `Display` string differs between ticks.
        for px in [5_050_000, 4_900_000, 5_100_000, 5_025_000, 4_950_000] {
            engine.update_price("BTC", px);
        }
        assert!(
            call_book.active_order_count() <= 2,
            "Days requote must replace, not accumulate (call): {}",
            call_book.active_order_count()
        );
        assert!(
            put_book.active_order_count() <= 2,
            "Days requote must replace, not accumulate (put): {}",
            put_book.active_order_count()
        );
        assert_eq!(
            engine.active_orders.read().len(),
            4,
            "tracking map must stay at two instruments * (bid+ask)"
        );
        // The reverse index tracks exactly the two instruments (call + put), one
        // entry each — proving it does not leak entries across ticks either.
        assert_eq!(
            engine.instrument_orders.read().len(),
            2,
            "reverse index must hold one entry per quoted instrument"
        );
    }

    #[test]
    fn test_kill_switch_cancels_days_orders_from_book_and_index() {
        // Issue #107: the cancel path (`cancel_order`, reached via the kill
        // switch) must key the book by the stored structural expiration, so a
        // `Days`-variant order is actually removed from the option book — the
        // old string-reparse turned a `Days` key into a `DateTime` key and left
        // the resting order behind.
        let engine = test_engine();
        let expiration = days_expiration();
        let underlying = engine.manager.get_or_create("BTC");
        let exp_book = underlying.get_or_create_expiration(expiration);
        let strike_book = exp_book.get_or_create_strike(5_000_000);
        let call_book = strike_book.get(OptionStyle::Call);
        let put_book = strike_book.get(OptionStyle::Put);

        engine.update_price("BTC", 5_000_000);
        assert_eq!(call_book.active_order_count(), 2);
        assert_eq!(put_book.active_order_count(), 2);

        // Kill switch: cancel everything.
        engine.set_enabled(false);

        assert_eq!(
            call_book.active_order_count(),
            0,
            "kill switch must cancel the Days call orders on the book"
        );
        assert_eq!(
            put_book.active_order_count(),
            0,
            "kill switch must cancel the Days put orders on the book"
        );
        assert!(
            engine.active_orders.read().is_empty(),
            "no tracked orders after the kill switch"
        );
        assert!(
            engine.instrument_orders.read().is_empty(),
            "reverse index cleared by the kill switch"
        );
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
        let ctx = RequoteContext {
            symbol: "ETH",
            expiration: &expiration,
            exp_display: &exp_str,
            exp_canonical: &exp_str,
            spot_cents: 350_000,
            config: &config,
        };
        engine.update_quote(&ctx, 300_000, OptionStyle::Call);
        engine.update_quote(&ctx, 300_000, OptionStyle::Put);
        engine.update_quote(&ctx, 400_000, OptionStyle::Call);
        assert_eq!(a_call.active_order_count(), 2);
        assert_eq!(a_put.active_order_count(), 2);
        assert_eq!(b_call.active_order_count(), 2);

        let a_put_before = instrument_ids(&engine, "ETH", 300_000, OptionStyle::Put);
        let b_call_before = instrument_ids(&engine, "ETH", 400_000, OptionStyle::Call);
        assert_eq!(a_put_before.len(), 2);
        assert_eq!(b_call_before.len(), 2);

        // Requote ONLY the strike-A call (new spot).
        let requote_ctx = RequoteContext {
            symbol: "ETH",
            expiration: &expiration,
            exp_display: &exp_str,
            exp_canonical: &exp_str,
            spot_cents: 351_000,
            config: &config,
        };
        engine.update_quote(&requote_ctx, 300_000, OptionStyle::Call);

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

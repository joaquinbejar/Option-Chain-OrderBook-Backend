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
    pub fn set_spread_multiplier(&self, multiplier: f64) {
        {
            let mut config = self.config.write();
            config.spread_multiplier = multiplier.clamp(0.1, 10.0);
        }
        self.broadcast_config_change();
        self.requote_all();
    }

    /// Updates the size scalar.
    pub fn set_size_scalar(&self, scalar: f64) {
        {
            let mut config = self.config.write();
            config.size_scalar = scalar.clamp(0.0, 1.0);
        }
        self.broadcast_config_change();
        self.requote_all();
    }

    /// Updates the directional skew.
    pub fn set_directional_skew(&self, skew: f64) {
        {
            let mut config = self.config.write();
            config.directional_skew = skew.clamp(-1.0, 1.0);
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
            for exp_entry in underlying_book.expirations().iter() {
                let expiration = exp_entry.key();
                let exp_book = exp_entry.value();

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

        let quote_params = self.quoter.generate_quote(&input);

        // Cancel existing orders and place new ones
        if let Ok(underlying_book) = self.manager.get(symbol)
            && let Ok(exp_book) = underlying_book.get_expiration(&expiration)
            && let Ok(strike_book) = exp_book.get_strike(strike)
        {
            let option_book = strike_book.get(style);

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
        use optionstratlib::prelude::pos_or_panic;

        // Try parsing as days first
        if let Ok(days) = exp_str.parse::<i32>() {
            return Ok(ExpirationDate::Days(pos_or_panic!(days as f64)));
        }

        // Try parsing as YYYYMMDD format
        if exp_str.len() == 8
            && let (Ok(year), Ok(month), Ok(day)) = (
                exp_str[0..4].parse::<i32>(),
                exp_str[4..6].parse::<u32>(),
                exp_str[6..8].parse::<u32>(),
            )
            && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            let time = NaiveTime::from_hms_opt(16, 0, 0).unwrap();
            let datetime = NaiveDateTime::new(date, time);
            let utc_datetime = Utc.from_utc_datetime(&datetime);
            return Ok(ExpirationDate::DateTime(utc_datetime));
        }

        Err(())
    }
}

//! Application state management.

use crate::api::websocket::OrderbookSubscriptionManager;
use crate::config::{AssetConfig, Config};
use crate::db::DatabasePool;
use crate::market_maker::MarketMakerEngine;
use crate::models::{LastTradeInfo, OrderInfo, PositionInfo};
use crate::simulation::PriceSimulator;
use dashmap::DashMap;
use option_chain_orderbook::orderbook::UnderlyingOrderBookManager;
use optionstratlib::ExpirationDate;
use std::sync::Arc;
use tracing::{info, warn};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// The underlying order book manager.
    pub manager: Arc<UnderlyingOrderBookManager>,
    /// Optional database pool.
    pub db: Option<DatabasePool>,
    /// Market maker engine.
    pub market_maker: Arc<MarketMakerEngine>,
    /// Price simulator.
    pub price_simulator: Option<Arc<PriceSimulator>>,
    /// Application configuration.
    pub config: Option<Config>,
    /// Storage for last trade information per symbol.
    pub last_trades: Arc<DashMap<String, LastTradeInfo>>,
    /// Storage for order information by order ID.
    pub orders: Arc<DashMap<String, OrderInfo>>,
    /// Storage for position information by symbol.
    pub positions: Arc<DashMap<String, PositionInfo>>,
    /// Orderbook subscription manager for WebSocket real-time updates.
    pub orderbook_subscriptions: Arc<OrderbookSubscriptionManager>,
}

impl AppState {
    /// Creates a new application state without database.
    #[must_use]
    pub fn new() -> Self {
        let manager = Arc::new(UnderlyingOrderBookManager::new());
        let market_maker = Arc::new(MarketMakerEngine::new(Arc::clone(&manager), None));

        Self {
            manager,
            db: None,
            market_maker,
            price_simulator: None,
            config: None,
            last_trades: Arc::new(DashMap::new()),
            orders: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
            orderbook_subscriptions: Arc::new(OrderbookSubscriptionManager::new()),
        }
    }

    /// Creates a new application state with database.
    #[must_use]
    pub fn with_database(db: DatabasePool) -> Self {
        let manager = Arc::new(UnderlyingOrderBookManager::new());
        let market_maker = Arc::new(MarketMakerEngine::new(
            Arc::clone(&manager),
            Some(db.clone()),
        ));

        Self {
            manager,
            db: Some(db),
            market_maker,
            price_simulator: None,
            config: None,
            last_trades: Arc::new(DashMap::new()),
            orders: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
            orderbook_subscriptions: Arc::new(OrderbookSubscriptionManager::new()),
        }
    }

    /// Creates a new application state from configuration.
    #[must_use]
    pub fn from_config(config: Config, db: Option<DatabasePool>) -> Self {
        let manager = Arc::new(UnderlyingOrderBookManager::new());

        // Initialize order books from config
        for asset in &config.assets {
            Self::initialize_asset_order_books(&manager, asset);
        }

        let market_maker = Arc::new(MarketMakerEngine::new(Arc::clone(&manager), db.clone()));

        // Set initial prices in market maker
        for asset in &config.assets {
            let price_cents = (asset.initial_price * 100.0) as u64;
            market_maker.update_price(&asset.symbol, price_cents);
        }

        // Create price simulator
        let price_simulator = Arc::new(PriceSimulator::new(
            config.assets.clone(),
            config.simulation.clone(),
        ));

        Self {
            manager,
            db,
            market_maker,
            price_simulator: Some(price_simulator),
            config: Some(config),
            last_trades: Arc::new(DashMap::new()),
            orders: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
            orderbook_subscriptions: Arc::new(OrderbookSubscriptionManager::new()),
        }
    }

    /// Initializes order books for an asset based on configuration.
    fn initialize_asset_order_books(manager: &UnderlyingOrderBookManager, asset: &AssetConfig) {
        // Create underlying
        let underlying = manager.get_or_create(&asset.symbol);
        info!("Created underlying: {}", asset.symbol);

        // Generate strikes
        let strikes = asset.generate_strikes();

        // Create expirations and strikes
        for exp_str in &asset.expirations {
            let expiration = match Self::parse_expiration(exp_str) {
                Some(e) => e,
                None => {
                    warn!("Invalid expiration format: {}", exp_str);
                    continue;
                }
            };

            let exp_book = underlying.get_or_create_expiration(expiration);
            info!("Created expiration {} for {}", exp_str, asset.symbol);

            // Create strikes
            for &strike in &strikes {
                drop(exp_book.get_or_create_strike(strike));
            }

            info!(
                "Created {} strikes for {}/{}",
                strikes.len(),
                asset.symbol,
                exp_str
            );
        }
    }

    /// Parses an expiration string (YYYYMMDD) into ExpirationDate.
    fn parse_expiration(exp_str: &str) -> Option<ExpirationDate> {
        if exp_str.len() != 8 {
            return None;
        }

        let year: i32 = exp_str[0..4].parse().ok()?;
        let month: u32 = exp_str[4..6].parse().ok()?;
        let day: u32 = exp_str[6..8].parse().ok()?;

        let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
        let datetime = date.and_hms_opt(16, 0, 0)?;
        let utc_datetime =
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc);

        Some(ExpirationDate::DateTime(utc_datetime))
    }

    /// Removes filled or canceled orders older than the specified age.
    ///
    /// # Arguments
    /// * `max_age_secs` - Maximum age in seconds for retention.
    ///
    /// # Returns
    /// Number of orders removed.
    pub fn cleanup_old_orders(&self, max_age_secs: u64) -> usize {
        let threshold = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);

        // Identify keys to remove first to avoid deadlock or long holding of locks
        // Dashmap creates deadlocks if you hold a read reference and try to remove.
        // We collect keys first.
        let keys_to_remove: Vec<String> = self
            .orders
            .iter()
            .filter_map(|entry| {
                let order = entry.value();
                // Check status
                if order.status == crate::models::OrderStatus::Filled
                    || order.status == crate::models::OrderStatus::Canceled
                {
                    // Check age
                    let updated = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                        order.updated_at_ms as i64,
                    );
                    if let Some(updated_dt) = updated
                        && updated_dt < threshold
                    {
                        return Some(entry.key().clone());
                    }
                }
                None
            })
            .collect();

        let count = keys_to_remove.len();
        if count > 0 {
            info!(
                "Running order cleanup. Found {} old orders to remove",
                count
            );
            for key in keys_to_remove {
                self.orders.remove(&key);
            }
        }

        count
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{OrderSide, OrderStatus, OrderTimeInForce};

    #[test]
    fn test_cleanup_old_orders() {
        let state = AppState::new();
        let now = chrono::Utc::now();
        let old_time = now - chrono::Duration::seconds(1000);

        // 1. Active order (should not be removed)
        let active_order = OrderInfo {
            order_id: "active1".to_string(),
            symbol: "BTC".to_string(),
            underlying: "BTC".to_string(),
            expiration: "20251231".to_string(),
            strike: 100000,
            style: "call".to_string(),
            side: OrderSide::Buy,
            price: 50000,
            original_quantity: 1,
            remaining_quantity: 1,
            filled_quantity: 0,
            status: OrderStatus::Active,
            time_in_force: OrderTimeInForce::Gtc,
            created_at_ms: old_time.timestamp_millis() as u64,
            updated_at_ms: old_time.timestamp_millis() as u64,
            fills: vec![],
        };
        state.orders.insert("active1".to_string(), active_order);

        // 2. Old filled order (should be removed)
        let filled_order = OrderInfo {
            order_id: "filled1".to_string(),
            symbol: "BTC".to_string(),
            underlying: "BTC".to_string(),
            expiration: "20251231".to_string(),
            strike: 100000,
            style: "call".to_string(),
            side: OrderSide::Buy,
            price: 50000,
            original_quantity: 1,
            remaining_quantity: 0,
            filled_quantity: 1,
            status: OrderStatus::Filled,
            time_in_force: OrderTimeInForce::Gtc,
            created_at_ms: old_time.timestamp_millis() as u64,
            updated_at_ms: old_time.timestamp_millis() as u64,
            fills: vec![],
        };
        state.orders.insert("filled1".to_string(), filled_order);

        // 3. Recent filled order (should not be removed yet)
        let recent_filled = OrderInfo {
            order_id: "filled_recent".to_string(),
            symbol: "BTC".to_string(),
            underlying: "BTC".to_string(),
            expiration: "20251231".to_string(),
            strike: 100000,
            style: "call".to_string(),
            side: OrderSide::Buy,
            price: 50000,
            original_quantity: 1,
            remaining_quantity: 0,
            filled_quantity: 1,
            status: OrderStatus::Filled,
            time_in_force: OrderTimeInForce::Gtc,
            created_at_ms: now.timestamp_millis() as u64,
            updated_at_ms: now.timestamp_millis() as u64,
            fills: vec![],
        };
        state
            .orders
            .insert("filled_recent".to_string(), recent_filled);

        // Run cleanup with 500s retention
        let removed = state.cleanup_old_orders(500);

        assert_eq!(removed, 1);
        assert!(state.orders.contains_key("active1")); // Active kept
        assert!(!state.orders.contains_key("filled1")); // Old filled removed
        assert!(state.orders.contains_key("filled_recent")); // Recent filled kept
    }
}

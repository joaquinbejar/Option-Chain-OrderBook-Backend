//! Application state management.

use crate::config::{AssetConfig, Config};
use crate::db::DatabasePool;
use crate::market_maker::MarketMakerEngine;
use crate::models::LastTradeInfo;
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
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

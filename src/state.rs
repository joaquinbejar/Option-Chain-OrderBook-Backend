//! Application state management.

use crate::db::DatabasePool;
use crate::market_maker::MarketMakerEngine;
use option_chain_orderbook::orderbook::UnderlyingOrderBookManager;
use std::sync::Arc;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// The underlying order book manager.
    pub manager: Arc<UnderlyingOrderBookManager>,
    /// Optional database pool.
    pub db: Option<DatabasePool>,
    /// Market maker engine.
    pub market_maker: Arc<MarketMakerEngine>,
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
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

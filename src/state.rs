//! Application state management.

use option_chain_orderbook::orderbook::UnderlyingOrderBookManager;
use std::sync::Arc;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// The underlying order book manager.
    pub manager: Arc<UnderlyingOrderBookManager>,
}

impl AppState {
    /// Creates a new application state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            manager: Arc::new(UnderlyingOrderBookManager::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

//! Application state management.

use crate::api::websocket::OrderbookSubscriptionManager;
use crate::auth::JwtAuth;
use crate::config::{AssetConfig, Config};
use crate::db::DatabasePool;
use crate::market_maker::MarketMakerEngine;
use crate::models::{ExecutionInfo, LastTradeInfo, OrderInfo, OrderbookSnapshotInfo, PositionInfo};
use crate::ohlc::OhlcAggregator;
use crate::simulation::PriceSimulator;
use dashmap::DashMap;
use option_chain_orderbook::orderbook::UnderlyingOrderBookManager;
use optionstratlib::ExpirationDate;
use std::sync::Arc;
use tracing::{info, warn};

/// A stored orderbook snapshot: the creation time plus the per-orderbook
/// entries.
///
/// The creation timestamp is kept at the snapshot level (not derived from the
/// entries) so that a snapshot with no entries — e.g. no resting orders, or
/// every book failed to serialize — still orders correctly for retention
/// eviction and listing instead of sorting as the epoch.
#[derive(Debug, Clone)]
pub struct StoredSnapshot {
    /// Creation timestamp in milliseconds since the Unix epoch.
    pub created_at_ms: u64,
    /// Per-orderbook snapshot entries.
    pub infos: Vec<OrderbookSnapshotInfo>,
}

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
    /// OHLC candlestick data aggregator.
    pub ohlc_aggregator: Arc<OhlcAggregator>,
    /// JWT authentication core (signing/verification keys + rate limiter).
    pub auth: Arc<JwtAuth>,
    /// Operator bootstrap secret for the token-issuance endpoint. `None` disables
    /// `POST /api/v1/auth/token` (tokens must then be minted out-of-band).
    pub bootstrap_secret: Option<String>,
    /// Whether a trusted reverse proxy sits in front of the server. When `false`
    /// (the secure default), the unauthenticated token endpoint rate-limits by
    /// the socket peer address and ignores client-supplied forwarding headers;
    /// when `true`, `X-Forwarded-For` / `X-Real-IP` is honored (issue #48).
    pub trust_proxy: bool,
    /// Storage for execution reports by execution ID.
    pub executions: Arc<DashMap<String, ExecutionInfo>>,
    /// Storage for orderbook snapshots by snapshot ID.
    pub snapshots: Arc<DashMap<String, StoredSnapshot>>,
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
            ohlc_aggregator: Arc::new(OhlcAggregator::new()),
            auth: Arc::new(JwtAuth::dev()),
            bootstrap_secret: None,
            trust_proxy: false,
            executions: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
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
            ohlc_aggregator: Arc::new(OhlcAggregator::new()),
            auth: Arc::new(JwtAuth::dev()),
            bootstrap_secret: None,
            trust_proxy: false,
            executions: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
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

        // Set initial prices in market maker, rounding dollars→cents through the
        // single canonical helper. A non-finite or out-of-range price is logged
        // and that asset is skipped (defensive: prices are already validated by
        // `Config::validate`) rather than seeded with a truncated value.
        for asset in &config.assets {
            match crate::config::dollars_to_cents(asset.initial_price) {
                Some(price_cents) => market_maker.update_price(&asset.symbol, price_cents),
                None => warn!(
                    symbol = %asset.symbol,
                    initial_price = asset.initial_price,
                    "skipping initial price seed: non-finite or out-of-range value"
                ),
            }
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
            ohlc_aggregator: Arc::new(OhlcAggregator::new()),
            auth: Arc::new(JwtAuth::dev()),
            bootstrap_secret: None,
            trust_proxy: false,
            executions: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
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
        // `len()` is a byte length, so the `is_ascii()` guard ensures every byte
        // is a char boundary before slicing — an 8-byte multibyte string (e.g.
        // `"12345é7"`) returns `None` instead of panicking on a non-boundary slice.
        if exp_str.len() != 8 || !exp_str.is_ascii() {
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

    /// Maximum number of orderbook snapshots retained in memory.
    ///
    /// Creating a snapshot beyond this cap evicts the oldest retained snapshot
    /// (by creation timestamp), so snapshot storage stays bounded regardless of
    /// how many `POST /api/v1/admin/snapshot` requests arrive.
    pub const MAX_RETAINED_SNAPSHOTS: usize = 16;

    /// Inserts a snapshot and enforces the retention bound.
    ///
    /// After the insert, while more than
    /// [`MAX_RETAINED_SNAPSHOTS`](Self::MAX_RETAINED_SNAPSHOTS) snapshots are
    /// retained, the oldest snapshot (by creation timestamp, tie-broken by
    /// snapshot ID for determinism) is evicted.
    pub fn insert_snapshot_bounded(&self, snapshot_id: String, snapshot: StoredSnapshot) {
        self.snapshots.insert(snapshot_id, snapshot);

        while self.snapshots.len() > Self::MAX_RETAINED_SNAPSHOTS {
            // Dashmap deadlocks if you remove while holding an iter reference,
            // so pick the eviction victim first, then remove.
            let oldest = self
                .snapshots
                .iter()
                .map(|entry| (entry.value().created_at_ms, entry.key().clone()))
                .min();

            let Some((created_at_ms, key)) = oldest else {
                break;
            };
            warn!(
                snapshot_id = %key,
                created_at_ms,
                cap = Self::MAX_RETAINED_SNAPSHOTS,
                "snapshot retention cap exceeded; evicting oldest snapshot"
            );
            self.snapshots.remove(&key);
        }
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

    fn stored_snapshot(id: &str, created_at: u64) -> StoredSnapshot {
        StoredSnapshot {
            created_at_ms: created_at,
            infos: vec![OrderbookSnapshotInfo {
                snapshot_id: id.to_string(),
                underlying: "BTC".to_string(),
                expiration: "20251231".to_string(),
                strike: 100_000,
                style: "call".to_string(),
                order_count: 1,
                bid_levels: 1,
                ask_levels: 1,
                data: "{}".to_string(),
                created_at,
            }],
        }
    }

    #[test]
    fn test_insert_snapshot_bounded_evicts_oldest_past_cap() {
        let state = AppState::new();
        let total = AppState::MAX_RETAINED_SNAPSHOTS + 2;

        for i in 0..total {
            let id = format!("snap-{i:03}");
            state.insert_snapshot_bounded(id.clone(), stored_snapshot(&id, 1_000 + i as u64));
        }

        assert_eq!(
            state.snapshots.len(),
            AppState::MAX_RETAINED_SNAPSHOTS,
            "retention must be capped at MAX_RETAINED_SNAPSHOTS"
        );
        // The two oldest snapshots were evicted; the newest cap-many remain.
        assert!(!state.snapshots.contains_key("snap-000"));
        assert!(!state.snapshots.contains_key("snap-001"));
        assert!(state.snapshots.contains_key("snap-002"));
        assert!(
            state
                .snapshots
                .contains_key(&format!("snap-{:03}", total - 1))
        );
    }

    #[test]
    fn test_insert_snapshot_bounded_under_cap_keeps_everything() {
        let state = AppState::new();

        for i in 0..AppState::MAX_RETAINED_SNAPSHOTS {
            let id = format!("snap-{i:03}");
            state.insert_snapshot_bounded(id.clone(), stored_snapshot(&id, 1_000 + i as u64));
        }

        assert_eq!(state.snapshots.len(), AppState::MAX_RETAINED_SNAPSHOTS);
        assert!(state.snapshots.contains_key("snap-000"));
    }

    #[test]
    fn test_insert_snapshot_bounded_empty_snapshot_keeps_its_timestamp() {
        let state = AppState::new();

        // Fill to the cap, then create a *newer* snapshot with no entries.
        // Its snapshot-level timestamp must be honored: the oldest non-empty
        // snapshot is evicted, never the fresh empty one (which would leave
        // the caller holding a phantom snapshot ID).
        for i in 0..AppState::MAX_RETAINED_SNAPSHOTS {
            let id = format!("snap-{i:03}");
            state.insert_snapshot_bounded(id.clone(), stored_snapshot(&id, 1_000 + i as u64));
        }
        state.insert_snapshot_bounded(
            "empty-newest".to_string(),
            StoredSnapshot {
                created_at_ms: 9_999,
                infos: Vec::new(),
            },
        );

        assert_eq!(state.snapshots.len(), AppState::MAX_RETAINED_SNAPSHOTS);
        assert!(state.snapshots.contains_key("empty-newest"));
        assert!(!state.snapshots.contains_key("snap-000"));
    }

    #[test]
    fn test_parse_expiration_accepts_yyyymmdd() {
        let exp = AppState::parse_expiration("20251231").expect("valid YYYYMMDD must parse");
        assert!(matches!(exp, ExpirationDate::DateTime(_)));
    }

    #[test]
    fn test_parse_expiration_rejects_multibyte_eight_bytes() {
        // `"12345é7"` is 8 bytes ('é' is 2 bytes) but not on char boundaries at
        // indices 4/6. Byte slicing would panic; the char-safe guard must return
        // `None` instead of panicking.
        let multibyte = "12345é7";
        assert_eq!(multibyte.len(), 8, "fixture must be exactly 8 bytes");
        assert!(AppState::parse_expiration(multibyte).is_none());
    }

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

    #[test]
    fn test_from_config_seeds_rounded_initial_price() {
        // A `.999` initial price must be ROUNDED to the nearest cent when seeded
        // into the market maker (10100), not truncated downward (10099) as the
        // old `as u64` cast did.
        let config = Config {
            assets: vec![AssetConfig {
                symbol: "RND".to_string(),
                name: "Rounding".to_string(),
                initial_price: 100.999,
                volatility: 0.2,
                drift: 0.0,
                expirations: vec!["20251231".to_string()],
                num_strikes: 2,
                strike_spacing: 10.0,
            }],
            ..Config::default()
        };

        let state = AppState::from_config(config, None);
        assert_eq!(state.market_maker.get_price("RND"), Some(10100));
    }
}

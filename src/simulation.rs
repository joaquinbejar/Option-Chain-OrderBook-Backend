//! Price simulation service using OptionStratLib random walk models.

use crate::config::{AssetConfig, SimulationConfig, WalkTypeConfig};
use crate::market_maker::MarketMakerEngine;
use optionstratlib::prelude::ExpirationDate;
use optionstratlib::prelude::Positive;
use optionstratlib::prelude::TimeFrame;
use optionstratlib::prelude::convert_time_frame;
use optionstratlib::prelude::{Step, Xstep, Ystep};
use optionstratlib::prelude::{WalkParams, WalkType, WalkTypeAble};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::AddAssign;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Number of steps in a generated price walk (30 days expressed in minutes).
///
/// A full regeneration builds this many steps, which is why it is offloaded to a
/// blocking task and never performed while a lock is held.
const WALK_STEPS: usize = 43_200;

/// Error returned when a price path cannot be generated from invalid inputs.
///
/// Each variant carries the offending value so a caller can log a `WARN` and skip
/// the misconfigured asset instead of panicking the whole simulation.
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    /// The initial price was not a valid, finite, positive value.
    #[error("invalid initial price: {0}")]
    InvalidInitialPrice(f64),
    /// The volatility was not a valid, finite, positive value.
    #[error("invalid volatility: {0}")]
    InvalidVolatility(f64),
}

/// Price update event broadcast to subscribers.
#[derive(Debug, Clone)]
pub struct PriceUpdate {
    /// Asset symbol.
    pub symbol: String,
    /// Current price in cents.
    pub price_cents: u64,
    /// Previous price in cents.
    pub previous_price_cents: u64,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// Walker implementation for OptionStratLib simulation.
#[derive(Clone)]
struct Walker;

impl Walker {
    fn new() -> Self {
        Walker
    }
}

impl<X, Y> WalkTypeAble<X, Y> for Walker
where
    X: Copy + Into<Positive> + AddAssign + Display,
    Y: Into<Positive> + Display + Clone,
{
}

/// Generates price values from walk parameters using OptionStratLib.
/// Returns only the y-values (prices) as f64 for thread-safety.
fn generate_price_path(
    initial_price: f64,
    volatility: f64,
    drift: f64,
    walk_type_config: &WalkTypeConfig,
    n_steps: usize,
) -> Result<Vec<f64>, SimulationError> {
    let initial = Positive::new(initial_price)
        .map_err(|_| SimulationError::InvalidInitialPrice(initial_price))?;
    let vol =
        Positive::new(volatility).map_err(|_| SimulationError::InvalidVolatility(volatility))?;
    let drift_dec = Decimal::try_from(drift).unwrap_or(dec!(0.0));
    let days = Positive::THIRTY;

    let walk_type = match walk_type_config {
        WalkTypeConfig::GeometricBrownian => WalkType::GeometricBrownian {
            dt: convert_time_frame(Positive::ONE / days, &TimeFrame::Minute, &TimeFrame::Day),
            drift: drift_dec,
            volatility: vol,
        },
        WalkTypeConfig::MeanReverting => WalkType::MeanReverting {
            dt: convert_time_frame(Positive::ONE / days, &TimeFrame::Minute, &TimeFrame::Day),
            volatility: vol,
            // 0.5 is a compile-time literal that is always a valid Positive.
            speed: Positive::new(0.5).expect("0.5 is a valid positive constant"),
            mean: initial,
        },
        WalkTypeConfig::JumpDiffusion => WalkType::JumpDiffusion {
            dt: convert_time_frame(Positive::ONE / days, &TimeFrame::Minute, &TimeFrame::Day),
            drift: drift_dec,
            volatility: vol,
            // 0.1 is a compile-time literal that is always a valid Positive.
            intensity: Positive::new(0.1).expect("0.1 is a valid positive constant"),
            jump_mean: dec!(0.0),
            // 0.05 is a compile-time literal that is always a valid Positive.
            jump_volatility: Positive::new(0.05).expect("0.05 is a valid positive constant"),
        },
    };

    let walk_params: WalkParams<Positive, Positive> = WalkParams {
        size: n_steps,
        init_step: Step {
            x: Xstep::new(Positive::ONE, TimeFrame::Minute, ExpirationDate::Days(days)),
            y: Ystep::new(0, initial),
        },
        walk_type,
        walker: Box::new(Walker::new()),
    };

    // Generate y-values using the walker
    let y_steps = match &walk_params.walk_type {
        WalkType::Brownian { .. } => walk_params
            .walker
            .brownian(&walk_params)
            .unwrap_or_default(),
        WalkType::GeometricBrownian { .. } => walk_params
            .walker
            .geometric_brownian(&walk_params)
            .unwrap_or_default(),
        WalkType::LogReturns { .. } => walk_params
            .walker
            .log_returns(&walk_params)
            .unwrap_or_default(),
        WalkType::MeanReverting { .. } => walk_params
            .walker
            .mean_reverting(&walk_params)
            .unwrap_or_default(),
        WalkType::JumpDiffusion { .. } => walk_params
            .walker
            .jump_diffusion(&walk_params)
            .unwrap_or_default(),
        WalkType::Garch { .. } => walk_params.walker.garch(&walk_params).unwrap_or_default(),
        WalkType::Heston { .. } => walk_params.walker.heston(&walk_params).unwrap_or_default(),
        _ => walk_params
            .walker
            .geometric_brownian(&walk_params)
            .unwrap_or_default(),
    };

    // Convert Positive values to f64
    Ok(y_steps.into_iter().map(|p: Positive| p.to_f64()).collect())
}

/// State for each asset's random walk simulation (thread-safe).
struct AssetSimulation {
    /// Current step index in the random walk.
    current_index: usize,
    /// Generated price path (f64 values).
    prices: Vec<f64>,
    /// When `true`, the most recent regeneration produced an empty or too-short
    /// path. The walk is left dormant (the last price is reused) instead of
    /// re-running the heavy generation every tick. Comprehensive recovery of a
    /// dormant walk is tracked separately (issue #71).
    unproducible: bool,
}

/// Outcome of advancing a single asset's walk by one step under a short lock.
///
/// The heavy regeneration is deliberately NOT performed inside the lock: when the
/// walk is exhausted this returns [`StepOutcome::NeedsRegen`] so the caller can
/// offload generation to a blocking task before re-acquiring the lock to store
/// the result.
enum StepOutcome {
    /// A fresh price (in dollars) is available at the new index.
    Price(f64),
    /// The walk is exhausted; regeneration is required off the async thread.
    NeedsRegen,
    /// The walk is dormant (`unproducible`); the last price should be reused.
    Dormant,
    /// No simulation is registered for this symbol.
    NoSim,
}

/// Price simulation service that generates realistic price movements using OptionStratLib.
pub struct PriceSimulator {
    /// Current prices for each asset (in cents).
    prices: Arc<RwLock<HashMap<String, u64>>>,
    /// Asset configurations.
    assets: Vec<AssetConfig>,
    /// Simulation configuration.
    config: SimulationConfig,
    /// Broadcast channel for price updates.
    price_tx: broadcast::Sender<PriceUpdate>,
    /// Random walk state per asset (thread-safe).
    simulations: RwLock<HashMap<String, AssetSimulation>>,
}

impl PriceSimulator {
    /// Creates a new price simulator.
    ///
    /// # Arguments
    /// * `assets` - List of asset configurations.
    /// * `config` - Simulation configuration.
    #[must_use]
    pub fn new(assets: Vec<AssetConfig>, config: SimulationConfig) -> Self {
        let mut initial_prices = HashMap::new();
        let mut simulations = HashMap::new();
        let n_steps = WALK_STEPS; // 30 days in minutes

        for asset in &assets {
            // Store prices in cents through the single canonical rounding helper.
            // A non-finite or out-of-range initial price is logged and the seed
            // is skipped rather than truncated to a corrupt value.
            match crate::config::dollars_to_cents(asset.initial_price) {
                Some(price_cents) => {
                    initial_prices.insert(asset.symbol.clone(), price_cents);
                }
                None => warn!(
                    symbol = %asset.symbol,
                    initial_price = asset.initial_price,
                    "skipping initial price seed for misconfigured asset"
                ),
            }

            // Generate price path for this asset. A misconfigured asset (e.g. a
            // non-positive or out-of-range initial price/volatility) is logged and
            // skipped rather than panicking the whole simulation at startup.
            match generate_price_path(
                asset.initial_price,
                asset.volatility,
                asset.drift,
                &config.walk_type,
                n_steps,
            ) {
                Ok(prices) => {
                    simulations.insert(
                        asset.symbol.clone(),
                        AssetSimulation {
                            current_index: 0,
                            prices,
                            unproducible: false,
                        },
                    );
                }
                Err(e) => {
                    warn!(
                        symbol = %asset.symbol,
                        error = %e,
                        "skipping price simulation for misconfigured asset"
                    );
                }
            }
        }

        let (price_tx, _) = broadcast::channel(1024);

        Self {
            prices: Arc::new(RwLock::new(initial_prices)),
            assets,
            config,
            price_tx,
            simulations: RwLock::new(simulations),
        }
    }

    /// Returns a receiver for price updates.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<PriceUpdate> {
        self.price_tx.subscribe()
    }

    /// Gets the current price for an asset in cents.
    #[must_use]
    pub fn get_price(&self, symbol: &str) -> Option<u64> {
        self.prices.read().get(symbol).copied()
    }

    /// Gets all current prices.
    #[must_use]
    pub fn get_all_prices(&self) -> HashMap<String, u64> {
        self.prices.read().clone()
    }

    /// Sets the price for an asset (for external price feeds).
    ///
    /// # Arguments
    /// * `symbol` - Asset symbol.
    /// * `price_cents` - New price in cents.
    pub fn set_price(&self, symbol: &str, price_cents: u64) {
        let previous = {
            let mut prices = self.prices.write();
            let prev = prices.get(symbol).copied().unwrap_or(price_cents);
            prices.insert(symbol.to_string(), price_cents);
            prev
        };

        let update = PriceUpdate {
            symbol: symbol.to_string(),
            price_cents,
            previous_price_cents: previous,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let _ = self.price_tx.send(update);
    }

    /// Starts the price simulation loop.
    ///
    /// The loop runs until `shutdown_rx` observes a `true` value, at which point
    /// it breaks cleanly between ticks so the spawning task can be awaited during
    /// graceful shutdown rather than being dropped mid-iteration.
    ///
    /// # Arguments
    /// * `market_maker` - Optional market maker engine to notify of price changes.
    /// * `shutdown_rx` - Watch receiver that signals the loop to terminate when it
    ///   transitions to `true`.
    pub async fn run(
        self: Arc<Self>,
        market_maker: Option<Arc<MarketMakerEngine>>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        if !self.config.enabled {
            info!("Price simulation disabled");
            return;
        }

        info!(
            "Starting price simulation with {}ms interval using OptionStratLib",
            self.config.interval_ms
        );

        let mut ticker = interval(Duration::from_millis(self.config.interval_ms));

        loop {
            tokio::select! {
                // Shutdown requested between ticks: stop the loop cleanly.
                _ = shutdown_rx.changed() => {
                    info!("price simulation shutting down");
                    break;
                }
                // Periodic price-walk tick.
                _ = ticker.tick() => {
                    // Race the tick body against shutdown so a regeneration that
                    // is due this tick (offloaded via `spawn_blocking`) cannot
                    // delay a clean exit: if the signal flips mid-tick we drop
                    // the in-flight work and break.
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            info!("price simulation shutting down");
                            break;
                        }
                        () = self.process_tick(market_maker.as_ref()) => {}
                    }
                }
            }
        }
    }

    /// Processes one simulation tick: advances every asset's walk and notifies
    /// the market maker. Heavy walk regeneration is offloaded inside
    /// [`Self::get_next_price`]; this never holds a lock across an `.await`.
    async fn process_tick(&self, market_maker: Option<&Arc<MarketMakerEngine>>) {
        for asset in &self.assets {
            let new_price = self.get_next_price(&asset.symbol, asset).await;

            if let Some(mm) = market_maker {
                mm.update_price(&asset.symbol, new_price);
            }

            debug!(
                "Price update: {} = ${:.2}",
                asset.symbol,
                new_price as f64 / 100.0
            );
        }
    }

    /// Gets the next price from the random walk simulation.
    ///
    /// The common case (walk not exhausted) advances the index and reads the next
    /// value under a short lock that is released before any `f64`→cents rounding
    /// or broadcast. When the walk is exhausted the heavy ~43k-step regeneration
    /// is offloaded to a blocking task in [`Self::regenerate`] with NO lock held
    /// across the generation, so it never blocks the async runtime.
    async fn get_next_price(&self, symbol: &str, asset: &AssetConfig) -> u64 {
        match self.advance_step(symbol) {
            StepOutcome::Price(price_dollars) => self.store_walk_price(symbol, price_dollars),
            StepOutcome::NeedsRegen => self.regenerate(symbol, asset).await,
            StepOutcome::Dormant => {
                // The walk produced an empty/too-short path on its last
                // regeneration. Reuse the last known price instead of re-running
                // the heavy generation every tick (the storm guard).
                self.get_price(symbol)
                    .or_else(|| crate::config::dollars_to_cents(asset.initial_price))
                    .unwrap_or(0)
            }
            StepOutcome::NoSim => {
                // No simulation path was registered (the asset was skipped at
                // construction). Fall back to the last stored price, then to the
                // canonical rounding of the configured initial price; both round
                // consistently. A non-finite/out-of-range price is logged and we
                // return 0 only as an unreachable last resort, never silently.
                self.get_price(symbol)
                    .or_else(|| crate::config::dollars_to_cents(asset.initial_price))
                    .unwrap_or_else(|| {
                        warn!(
                            symbol = %symbol,
                            initial_price = asset.initial_price,
                            "no simulation registered and initial price invalid; returning 0"
                        );
                        0
                    })
            }
        }
    }

    /// Advances one walk step under a short lock and classifies the result.
    ///
    /// This NEVER performs regeneration: when the walk is exhausted it returns
    /// [`StepOutcome::NeedsRegen`] so the heavy work can happen off the lock.
    fn advance_step(&self, symbol: &str) -> StepOutcome {
        let mut simulations = self.simulations.write();
        let Some(sim) = simulations.get_mut(symbol) else {
            return StepOutcome::NoSim;
        };

        // A dormant walk is left alone: no index advance, no regeneration.
        if sim.unproducible {
            return StepOutcome::Dormant;
        }

        sim.current_index += 1;

        if sim.current_index >= sim.prices.len() {
            return StepOutcome::NeedsRegen;
        }

        // Common case: read the next value while we still hold the lock, then
        // release it before any rounding / broadcast happens in the caller.
        match sim.prices.get(sim.current_index).copied() {
            Some(price_dollars) => StepOutcome::Price(price_dollars),
            // Unreachable given the bounds check above, but handled without a
            // panic: treat a missing index as exhaustion.
            None => StepOutcome::NeedsRegen,
        }
    }

    /// Rounds a walk value (dollars) to cents, stores it, and broadcasts it.
    ///
    /// Runs with NO `simulations` lock held. A non-finite/out-of-range value is
    /// logged and the last known price is reused rather than corrupting the cents.
    fn store_walk_price(&self, symbol: &str, price_dollars: f64) -> u64 {
        let price_cents =
            crate::config::dollars_to_cents(price_dollars.max(0.01)).unwrap_or_else(|| {
                let fallback = self.get_price(symbol).unwrap_or(0);
                warn!(
                    symbol = %symbol,
                    price_dollars,
                    fallback_cents = fallback,
                    "simulated price is non-finite or out of range; reusing last known price"
                );
                fallback
            });

        self.set_price(symbol, price_cents);
        price_cents
    }

    /// Regenerates an exhausted walk OFF the async thread.
    ///
    /// Control flow: (a) read the starting price under a short lock and release
    /// it; (b) run [`generate_price_path`] inside `spawn_blocking` with no lock
    /// held; (c) re-acquire the lock briefly to store the new path and reset the
    /// index. An empty/too-short result marks the walk dormant so the heavy
    /// generation does not run every tick. A join error is logged and the tick is
    /// skipped (the current price is reused) rather than panicking.
    async fn regenerate(&self, symbol: &str, asset: &AssetConfig) -> u64 {
        // (a) Starting point, under a short lock via `get_price`, released here.
        let current_price = self
            .get_price(symbol)
            .or_else(|| crate::config::dollars_to_cents(asset.initial_price))
            .unwrap_or_else(|| {
                warn!(
                    symbol = %symbol,
                    initial_price = asset.initial_price,
                    "no stored price and initial price invalid; restarting walk from 0"
                );
                0
            });
        let price_dollars = current_price as f64 / 100.0;

        // Owned, `'static` params for the blocking closure (no lock, no `self`).
        let volatility = asset.volatility;
        let drift = asset.drift;
        let walk_type = self.config.walk_type.clone();

        // (b) Heavy generation off the async runtime; NO lock held across it.
        let result = tokio::task::spawn_blocking(move || {
            generate_price_path(price_dollars, volatility, drift, &walk_type, WALK_STEPS)
        })
        .await;

        let prices = match result {
            Ok(Ok(prices)) => prices,
            Ok(Err(e)) => {
                // Invalid inputs: cheap to detect (no heavy walk ran). Mark the
                // walk dormant so we do not retry the generation every tick.
                warn!(
                    symbol = %symbol,
                    error = %e,
                    "failed to regenerate price path; marking walk dormant and reusing price"
                );
                self.mark_unproducible(symbol);
                return current_price;
            }
            Err(join_err) => {
                // The blocking task could not complete (extremely rare). Skip the
                // tick by reusing the current price; leave the walk schedulable.
                warn!(
                    symbol = %symbol,
                    error = %join_err,
                    "price path regeneration task failed to join; reusing current price"
                );
                return current_price;
            }
        };

        // Storm guard: an empty or too-short path would exhaust again next tick
        // and re-trigger generation forever. Mark the walk dormant instead.
        if prices.len() < 2 {
            error!(
                symbol = %symbol,
                len = prices.len(),
                "regenerated price path is empty or too short; marking walk dormant"
            );
            self.mark_unproducible(symbol);
            return current_price;
        }

        // (c) Store the new path and reset the index under a short lock, then read
        // the first walked value (index 0 is the initial seed).
        let next_dollars = {
            let mut simulations = self.simulations.write();
            match simulations.get_mut(symbol) {
                Some(sim) => {
                    sim.prices = prices;
                    sim.current_index = 1;
                    sim.prices.get(1).copied()
                }
                None => None,
            }
        };

        match next_dollars {
            Some(price_dollars) => self.store_walk_price(symbol, price_dollars),
            None => current_price,
        }
    }

    /// Marks a walk dormant under a short lock so it is not regenerated each tick.
    fn mark_unproducible(&self, symbol: &str) {
        if let Some(sim) = self.simulations.write().get_mut(symbol) {
            sim.unproducible = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_asset() -> AssetConfig {
        AssetConfig {
            symbol: "TEST".to_string(),
            name: "Test Asset".to_string(),
            initial_price: 100.0,
            volatility: 0.2,
            drift: 0.05,
            expirations: vec!["20251231".to_string()],
            num_strikes: 10,
            strike_spacing: 5.0,
        }
    }

    #[test]
    fn test_price_simulator_creation() {
        let assets = vec![test_asset()];
        let config = SimulationConfig::default();
        let simulator = PriceSimulator::new(assets, config);

        assert_eq!(simulator.get_price("TEST"), Some(10000)); // $100 = 10000 cents
    }

    #[test]
    fn test_set_price() {
        let assets = vec![test_asset()];
        let config = SimulationConfig::default();
        let simulator = PriceSimulator::new(assets, config);

        simulator.set_price("TEST", 15000);
        assert_eq!(simulator.get_price("TEST"), Some(15000));
    }

    #[test]
    fn test_generate_price_path() {
        let prices =
            generate_price_path(100.0, 0.2, 0.05, &WalkTypeConfig::GeometricBrownian, 1000)
                .expect("valid inputs produce a price path");

        assert!(!prices.is_empty());
        // First price should be close to initial
        assert!((prices[0] - 100.0).abs() < 1.0);
        // All prices should be positive
        assert!(prices.iter().all(|&p| p > 0.0));
    }

    #[test]
    fn test_generate_price_path_invalid_initial_price() {
        // A non-positive initial price must yield an error, not a panic.
        let result = generate_price_path(-1.0, 0.2, 0.05, &WalkTypeConfig::GeometricBrownian, 100);
        assert!(matches!(
            result,
            Err(SimulationError::InvalidInitialPrice(_))
        ));
    }

    #[test]
    fn test_generate_price_path_invalid_volatility() {
        // A non-positive volatility must yield an error, not a panic.
        let result =
            generate_price_path(100.0, -0.2, 0.05, &WalkTypeConfig::GeometricBrownian, 100);
        assert!(matches!(result, Err(SimulationError::InvalidVolatility(_))));
    }

    #[tokio::test]
    async fn test_new_skips_misconfigured_asset_without_panic() {
        // A misconfigured asset must be skipped at startup, not crash the server.
        let mut bad = test_asset();
        bad.initial_price = -5.0;
        let simulator = PriceSimulator::new(vec![bad], SimulationConfig::default());
        // No simulation path was registered, but the call did not panic and the
        // next-price lookup falls back gracefully (the `NoSim` branch).
        let price = simulator.get_next_price("TEST", &test_asset()).await;
        let _ = price;
    }

    #[tokio::test]
    async fn test_get_next_price() {
        let assets = vec![test_asset()];
        let config = SimulationConfig::default();
        let simulator = PriceSimulator::new(assets.clone(), config);

        let price1 = simulator.get_next_price("TEST", &assets[0]).await;
        let price2 = simulator.get_next_price("TEST", &assets[0]).await;

        // Prices should be positive
        assert!(price1 > 0);
        assert!(price2 > 0);
    }

    /// Helper: force an asset's walk to the exhausted state so the next
    /// `get_next_price` takes the offloaded regeneration path.
    fn exhaust_walk(simulator: &PriceSimulator, symbol: &str) {
        let mut sims = simulator.simulations.write();
        let sim = sims
            .get_mut(symbol)
            .expect("registered simulation for symbol");
        sim.current_index = sim.prices.len();
    }

    /// When a walk is exhausted, the next price MUST be produced via the
    /// offloaded (`spawn_blocking`) regeneration path WITHOUT holding the
    /// `simulations` lock across the heavy generation. Driving it under a bounded
    /// timeout proves the loop stays responsive: a lock held across the ~43k-step
    /// generation (or a busy-regen storm) would not complete this quickly.
    #[tokio::test]
    async fn test_exhausted_walk_regenerates_via_offload() {
        let assets = vec![test_asset()];
        let simulator = PriceSimulator::new(assets.clone(), SimulationConfig::default());

        exhaust_walk(&simulator, "TEST");

        let price = tokio::time::timeout(
            Duration::from_secs(5),
            simulator.get_next_price("TEST", &assets[0]),
        )
        .await
        .expect("offloaded regeneration completed within the timeout");

        assert!(price > 0, "regenerated walk must yield a positive price");

        // The walk was refilled and the index reset, so the very next tick reads
        // from the fresh path without another regeneration.
        let next = tokio::time::timeout(
            Duration::from_secs(1),
            simulator.get_next_price("TEST", &assets[0]),
        )
        .await
        .expect("subsequent tick reads from the refilled path quickly");
        assert!(next > 0);

        let sims = simulator.simulations.read();
        let sim = sims.get("TEST").expect("simulation still registered");
        assert!(
            !sim.unproducible,
            "a healthy walk must not be marked dormant"
        );
        assert!(sim.prices.len() >= 2, "walk should have been refilled");
    }

    /// A regeneration that cannot produce a usable path (here forced via an
    /// invalid volatility, which makes `generate_price_path` return `Err`) MUST
    /// mark the walk dormant and back off: subsequent ticks reuse the last price
    /// and never re-run the heavy generation. This is the storm guard — without
    /// it, `current_index >= len` every tick would regenerate forever.
    #[tokio::test]
    async fn test_unusable_regen_marks_dormant_and_backs_off() {
        let assets = vec![test_asset()];
        let simulator = PriceSimulator::new(assets, SimulationConfig::default());

        // An asset whose volatility is invalid forces the regen to fail.
        let mut bad = test_asset();
        bad.volatility = -1.0;

        exhaust_walk(&simulator, "TEST");

        // First exhausted tick: regen fails -> walk marked dormant, price reused.
        let reused = simulator.get_next_price("TEST", &bad).await;
        assert!(reused > 0, "a failed regen must reuse the last known price");

        {
            let sims = simulator.simulations.read();
            let sim = sims.get("TEST").expect("simulation still registered");
            assert!(
                sim.unproducible,
                "an unusable regeneration must mark the walk dormant"
            );
        }

        // Back-off: many subsequent ticks must complete near-instantly (the
        // dormant branch skips regeneration entirely — no per-tick 43k storm).
        let elapsed = tokio::time::timeout(Duration::from_secs(1), async {
            let start = std::time::Instant::now();
            for _ in 0..1000 {
                let p = simulator.get_next_price("TEST", &bad).await;
                assert!(p > 0);
            }
            start.elapsed()
        })
        .await
        .expect("dormant walk must not busy-regenerate (no hang / storm)");

        assert!(
            elapsed < Duration::from_secs(1),
            "1000 dormant ticks should be cheap, took {elapsed:?}"
        );

        // The walk stays dormant; the index is not advanced while dormant.
        let sims = simulator.simulations.read();
        let sim = sims.get("TEST").expect("simulation still registered");
        assert!(sim.unproducible, "walk must remain dormant after back-off");
    }

    /// The run loop MUST still observe the shutdown signal promptly even when a
    /// regeneration is due this tick: the offloaded generation is raced against
    /// the shutdown receiver, so the task returns within the bounded timeout
    /// instead of blocking until the heavy work finishes.
    #[tokio::test]
    async fn test_run_shuts_down_promptly_when_regen_due() {
        let assets = vec![test_asset()];
        let simulator = Arc::new(PriceSimulator::new(assets, SimulationConfig::default()));

        // Make a regeneration due on the very first tick.
        exhaust_walk(&simulator, "TEST");

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(Arc::clone(&simulator).run(None, rx));

        tx.send(true)
            .expect("watch receiver kept alive by the task");
        let joined = tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert!(
            joined.is_ok(),
            "run loop did not shut down promptly with a regeneration due"
        );
        joined
            .expect("did not time out")
            .expect("run task should not panic");
    }

    /// The run loop MUST terminate once the watch signal flips to `true`. Under
    /// the old unbounded loop this task would never return and the timeout below
    /// would elapse (the test would fail/hang).
    #[tokio::test]
    async fn test_run_terminates_on_shutdown_signal() {
        let assets = vec![test_asset()];
        let config = SimulationConfig::default();
        let simulator = Arc::new(PriceSimulator::new(assets, config));

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(simulator.run(None, rx));

        // Request shutdown and confirm the task returns well within the timeout.
        tx.send(true)
            .expect("watch receiver kept alive by the task");
        let joined = tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert!(
            joined.is_ok(),
            "price simulation run loop did not terminate after shutdown signal"
        );
        joined
            .expect("did not time out")
            .expect("run task should not panic");
    }

    /// A disabled simulation returns immediately regardless of the shutdown
    /// signal, so the task is never left detached.
    #[tokio::test]
    async fn test_run_returns_when_disabled() {
        let assets = vec![test_asset()];
        let config = SimulationConfig {
            enabled: false,
            ..SimulationConfig::default()
        };
        let simulator = Arc::new(PriceSimulator::new(assets, config));

        let (_tx, rx) = watch::channel(false);
        let handle = tokio::spawn(simulator.run(None, rx));

        let joined = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            joined.is_ok(),
            "disabled price simulation run loop did not return promptly"
        );
        joined
            .expect("did not time out")
            .expect("run task should not panic");
    }

    /// A `select!` over a watch shutdown receiver and a periodic interval (the
    /// shape used by the order-cleanup and rate-limit-sweep tasks in `main.rs`)
    /// MUST exit promptly when the signal flips, not on the next tick boundary.
    #[tokio::test]
    async fn test_select_loop_exits_on_signal() {
        let (tx, mut shutdown_rx) = watch::channel(false);

        let handle = tokio::spawn(async move {
            // A deliberately long interval: if the loop waited for a tick instead
            // of the signal, the timeout below would elapse.
            let mut ticker = interval(Duration::from_secs(3600));
            ticker.tick().await; // consume the immediate first tick
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => break,
                    _ = ticker.tick() => {}
                }
            }
        });

        tx.send(true)
            .expect("watch receiver kept alive by the task");
        let joined = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            joined.is_ok(),
            "select! loop did not exit promptly on shutdown signal"
        );
        joined
            .expect("did not time out")
            .expect("task should not panic");
    }
}

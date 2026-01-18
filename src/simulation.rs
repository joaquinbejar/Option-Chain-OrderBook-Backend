//! Price simulation service using OptionStratLib random walk models.

use crate::config::{AssetConfig, SimulationConfig, WalkTypeConfig};
use crate::market_maker::MarketMakerEngine;
use optionstratlib::prelude::ExpirationDate;
use optionstratlib::prelude::TimeFrame;
use optionstratlib::prelude::convert_time_frame;
use optionstratlib::prelude::{Positive, pos_or_panic};
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
use tokio::sync::broadcast;
use tokio::time::interval;
use tracing::{debug, info};

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
) -> Vec<f64> {
    let initial = pos_or_panic!(initial_price);
    let vol = pos_or_panic!(volatility);
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
            speed: pos_or_panic!(0.5),
            mean: initial,
        },
        WalkTypeConfig::JumpDiffusion => WalkType::JumpDiffusion {
            dt: convert_time_frame(Positive::ONE / days, &TimeFrame::Minute, &TimeFrame::Day),
            drift: drift_dec,
            volatility: vol,
            intensity: pos_or_panic!(0.1),
            jump_mean: dec!(0.0),
            jump_volatility: pos_or_panic!(0.05),
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
    y_steps.into_iter().map(|p: Positive| p.to_f64()).collect()
}

/// State for each asset's random walk simulation (thread-safe).
struct AssetSimulation {
    /// Current step index in the random walk.
    current_index: usize,
    /// Generated price path (f64 values).
    prices: Vec<f64>,
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
        let n_steps = 43_200; // 30 days in minutes

        for asset in &assets {
            // Store prices in cents
            let price_cents = (asset.initial_price * 100.0) as u64;
            initial_prices.insert(asset.symbol.clone(), price_cents);

            // Generate price path for this asset
            let prices = generate_price_path(
                asset.initial_price,
                asset.volatility,
                asset.drift,
                &config.walk_type,
                n_steps,
            );

            simulations.insert(
                asset.symbol.clone(),
                AssetSimulation {
                    current_index: 0,
                    prices,
                },
            );
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
    /// # Arguments
    /// * `market_maker` - Optional market maker engine to notify of price changes.
    pub async fn run(self: Arc<Self>, market_maker: Option<Arc<MarketMakerEngine>>) {
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
            ticker.tick().await;

            for asset in &self.assets {
                let new_price = self.get_next_price(&asset.symbol, asset);

                if let Some(ref mm) = market_maker {
                    mm.update_price(&asset.symbol, new_price);
                }

                debug!(
                    "Price update: {} = ${:.2}",
                    asset.symbol,
                    new_price as f64 / 100.0
                );
            }
        }
    }

    /// Gets the next price from the random walk simulation.
    fn get_next_price(&self, symbol: &str, asset: &AssetConfig) -> u64 {
        let mut simulations = self.simulations.write();
        let n_steps = 43_200;

        let sim = match simulations.get_mut(symbol) {
            Some(s) => s,
            None => return (asset.initial_price * 100.0) as u64,
        };

        // Advance to next step
        sim.current_index += 1;

        // If we've exhausted the walk, regenerate
        if sim.current_index >= sim.prices.len() {
            // Get current price as new starting point
            let current_price = self
                .get_price(symbol)
                .unwrap_or((asset.initial_price * 100.0) as u64);
            let price_dollars = current_price as f64 / 100.0;

            // Regenerate price path starting from current price
            sim.prices = generate_price_path(
                price_dollars,
                asset.volatility,
                asset.drift,
                &self.config.walk_type,
                n_steps,
            );
            sim.current_index = 1; // Start from step 1 (step 0 is initial)
        }

        // Get price from current step
        let price_dollars = sim
            .prices
            .get(sim.current_index)
            .copied()
            .unwrap_or(asset.initial_price);
        let price_cents = (price_dollars.max(0.01) * 100.0) as u64;

        // Update stored price
        self.set_price(symbol, price_cents);

        price_cents
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
            generate_price_path(100.0, 0.2, 0.05, &WalkTypeConfig::GeometricBrownian, 1000);

        assert!(!prices.is_empty());
        // First price should be close to initial
        assert!((prices[0] - 100.0).abs() < 1.0);
        // All prices should be positive
        assert!(prices.iter().all(|&p| p > 0.0));
    }

    #[test]
    fn test_get_next_price() {
        let assets = vec![test_asset()];
        let config = SimulationConfig::default();
        let simulator = PriceSimulator::new(assets.clone(), config);

        let price1 = simulator.get_next_price("TEST", &assets[0]);
        let price2 = simulator.get_next_price("TEST", &assets[0]);

        // Prices should be positive
        assert!(price1 > 0);
        assert!(price2 > 0);
    }
}

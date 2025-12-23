//! Configuration module for loading and parsing TOML configuration files.

use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Configuration error types.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read configuration file.
    #[error("failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    /// Failed to parse TOML configuration.
    #[error("failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),
    /// Invalid configuration value.
    #[error("invalid config value: {0}")]
    InvalidValue(String),
}

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Server configuration.
    pub server: ServerConfig,
    /// Price simulation configuration.
    pub simulation: SimulationConfig,
    /// List of configured assets.
    pub assets: Vec<AssetConfig>,
}

/// Server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host address to bind to.
    pub host: String,
    /// Port number to listen on.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

/// Price simulation configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SimulationConfig {
    /// Whether price simulation is enabled.
    pub enabled: bool,
    /// Update interval in milliseconds.
    pub interval_ms: u64,
    /// Type of random walk to use.
    pub walk_type: WalkTypeConfig,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 1000,
            walk_type: WalkTypeConfig::GeometricBrownian,
        }
    }
}

/// Walk type configuration for price simulation.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WalkTypeConfig {
    /// Geometric Brownian motion (log-normal).
    GeometricBrownian,
    /// Mean-reverting (Ornstein-Uhlenbeck).
    MeanReverting,
    /// Jump diffusion process.
    JumpDiffusion,
}

/// Asset configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetConfig {
    /// Asset symbol (e.g., "BTC", "ETH").
    pub symbol: String,
    /// Human-readable name.
    pub name: String,
    /// Initial price in dollars.
    pub initial_price: f64,
    /// Annualized volatility (0.0 to 1.0).
    pub volatility: f64,
    /// Drift (expected annual return).
    pub drift: f64,
    /// List of expiration dates in YYYYMMDD format.
    pub expirations: Vec<String>,
    /// Number of strikes per expiration.
    pub num_strikes: u32,
    /// Strike spacing in dollars.
    pub strike_spacing: f64,
}

impl AssetConfig {
    /// Generates strike prices centered around the initial price.
    ///
    /// # Returns
    /// Vector of strike prices in cents.
    #[must_use]
    pub fn generate_strikes(&self) -> Vec<u64> {
        let center = self.initial_price;
        let half_count = self.num_strikes / 2;
        let mut strikes = Vec::with_capacity(self.num_strikes as usize);

        for i in 0..self.num_strikes {
            let offset = (i as f64 - half_count as f64) * self.strike_spacing;
            let strike = (center + offset).max(self.strike_spacing);
            // Convert to cents
            strikes.push((strike * 100.0) as u64);
        }

        strikes
    }
}

impl Config {
    /// Loads configuration from a TOML file.
    ///
    /// # Arguments
    /// * `path` - Path to the configuration file.
    ///
    /// # Errors
    /// Returns error if file cannot be read or parsed.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Parses configuration from a TOML string.
    ///
    /// # Arguments
    /// * `content` - TOML content as string.
    ///
    /// # Errors
    /// Returns error if content cannot be parsed.
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration values.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.assets.is_empty() {
            return Err(ConfigError::InvalidValue(
                "at least one asset must be configured".to_string(),
            ));
        }

        for asset in &self.assets {
            if asset.symbol.is_empty() {
                return Err(ConfigError::InvalidValue(
                    "asset symbol cannot be empty".to_string(),
                ));
            }
            if asset.initial_price <= 0.0 {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} initial_price must be positive",
                    asset.symbol
                )));
            }
            if asset.volatility <= 0.0 || asset.volatility > 5.0 {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} volatility must be between 0 and 5",
                    asset.symbol
                )));
            }
            if asset.expirations.is_empty() {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} must have at least one expiration",
                    asset.symbol
                )));
            }
            if asset.num_strikes == 0 {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} num_strikes must be positive",
                    asset.symbol
                )));
            }
            if asset.strike_spacing <= 0.0 {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} strike_spacing must be positive",
                    asset.symbol
                )));
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            simulation: SimulationConfig::default(),
            assets: vec![AssetConfig {
                symbol: "BTC".to_string(),
                name: "Bitcoin".to_string(),
                initial_price: 100000.0,
                volatility: 0.65,
                drift: 0.05,
                expirations: vec!["20251231".to_string()],
                num_strikes: 50,
                strike_spacing: 1000.0,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_content = r#"
[server]
host = "127.0.0.1"
port = 3000

[simulation]
enabled = true
interval_ms = 500
walk_type = "geometric_brownian"

[[assets]]
symbol = "BTC"
name = "Bitcoin"
initial_price = 100000.0
volatility = 0.65
drift = 0.05
expirations = ["20251225", "20251231"]
num_strikes = 10
strike_spacing = 1000.0
"#;

        let config = Config::parse(toml_content).expect("should parse");
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert!(config.simulation.enabled);
        assert_eq!(config.simulation.interval_ms, 500);
        assert_eq!(
            config.simulation.walk_type,
            WalkTypeConfig::GeometricBrownian
        );
        assert_eq!(config.assets.len(), 1);
        assert_eq!(config.assets[0].symbol, "BTC");
        assert_eq!(config.assets[0].expirations.len(), 2);
    }

    #[test]
    fn test_generate_strikes() {
        let asset = AssetConfig {
            symbol: "TEST".to_string(),
            name: "Test".to_string(),
            initial_price: 100.0,
            volatility: 0.2,
            drift: 0.0,
            expirations: vec!["20251231".to_string()],
            num_strikes: 5,
            strike_spacing: 10.0,
        };

        let strikes = asset.generate_strikes();
        assert_eq!(strikes.len(), 5);
        // Strikes should be centered around 100: 80, 90, 100, 110, 120
        assert_eq!(strikes, vec![8000, 9000, 10000, 11000, 12000]);
    }

    #[test]
    fn test_validation_empty_assets() {
        let config = Config {
            server: ServerConfig::default(),
            simulation: SimulationConfig::default(),
            assets: vec![],
        };
        assert!(config.validate().is_err());
    }
}

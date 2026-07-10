//! Configuration module for loading and parsing TOML configuration files.

use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Maximum accepted dollar price anywhere a dollar amount is converted to cents.
///
/// This is the ONE canonical upper bound for the server's dollar→cents
/// conversions (see [`dollars_to_cents`]): asset `initial_price` validation,
/// startup price seeding, strike generation, the price simulation, and the live
/// `POST /api/v1/prices` insert all share it. Bounding the input keeps the cents
/// result (`<= MAX_INITIAL_PRICE * 100`) well within `u64`/`u128`/`i64` range, so
/// a conversion can never overflow or wrap. A trillion dollars is far beyond any
/// real underlying yet leaves huge headroom.
pub const MAX_INITIAL_PRICE: f64 = 1e12;

/// Maximum accepted asset `volatility` (annualized, as a fraction).
pub const MAX_VOLATILITY: f64 = 5.0;

/// Maximum accepted relative expiration, in days (~100 years).
///
/// A day count beyond this is structurally absurd for an option expiration
/// and, if allowed through, overflows chrono's date arithmetic inside the
/// upstream `ExpirationDate → date` conversion — a request-reachable panic
/// (issue #136: restore of a legacy shadow-book snapshot carried
/// `"+574970603"`, which parses as an `i32` day count). Every
/// `parse_expiration` site rejects day counts above this bound.
pub const MAX_EXPIRATION_DAYS: i32 = 36_500;

/// Converts a dollar amount to integer cents using the single, canonical
/// rounding policy for the whole server.
///
/// Rounds to the nearest cent (`f64::round`, i.e. half away from zero), so
/// `0.07` becomes `7` cents and `100.999` becomes `10100` — never the truncated
/// `6` / `10099` produced by a bare `(value * 100.0) as u64` cast, which biases
/// every price downward.
///
/// Returns `None` for any value that is not a usable price — non-finite
/// (`NaN` / `±∞`), negative, or greater than [`MAX_INITIAL_PRICE`] — so a bad
/// `f64` can never wrap, saturate, or overflow. A `Some` result is only ever
/// produced from a verified, in-range value and lies in
/// `[0, MAX_INITIAL_PRICE * 100]`, far below `u64::MAX`.
///
/// Every dollar **input**→cents conversion (config seed, simulation, strike
/// generation, and the live `POST /api/v1/prices` insert) routes through this
/// helper so the rounding policy is single-sourced; the transport layer
/// (`api::controls::dollars_to_cents`) wraps it to attach granular, per-reason
/// error messages. The market-maker quoter's theoretical-value→cents conversion
/// is separate.
#[must_use]
pub(crate) fn dollars_to_cents(value: f64) -> Option<u64> {
    if !value.is_finite() || !(0.0..=MAX_INITIAL_PRICE).contains(&value) {
        return None;
    }
    // Safe: `value` is finite and in `[0, MAX_INITIAL_PRICE]`, so the rounded
    // cents value lies in `[0, MAX_INITIAL_PRICE * 100]`, far below `u64::MAX`.
    // The cast cannot overflow or wrap.
    Some((value * 100.0).round() as u64)
}

/// Default token issuer (`iss` claim) when none is configured.
pub const DEFAULT_ISSUER: &str = "option-chain-orderbook-backend";

/// Default minted-token lifetime in seconds (1 hour).
pub const DEFAULT_TOKEN_TTL_SECS: u64 = 3600;

/// Built-in DEV private-key path (relative to the crate root).
pub const DEV_PRIVATE_KEY_PATH: &str = "tests/fixtures/dev-private-key.pem";

/// Built-in DEV x509 certificate path (relative to the crate root).
pub const DEV_CERT_PATH: &str = "tests/fixtures/dev-cert.pem";

/// Environment variable overriding the private-key PEM path.
pub const ENV_PRIVATE_KEY_PATH: &str = "AUTH_PRIVATE_KEY_PATH";

/// Environment variable overriding the x509 certificate PEM path.
pub const ENV_CERT_PATH: &str = "AUTH_CERT_PATH";

/// Environment variable overriding the token issuer.
pub const ENV_ISSUER: &str = "AUTH_ISSUER";

/// Environment variable overriding the default token TTL (seconds).
pub const ENV_DEFAULT_TTL_SECS: &str = "AUTH_DEFAULT_TTL_SECS";

/// Environment variable holding the operator bootstrap secret. When unset, the
/// `POST /api/v1/auth/token` endpoint is disabled.
pub const ENV_BOOTSTRAP_SECRET: &str = "AUTH_BOOTSTRAP_SECRET";

/// Environment variable enabling reverse-proxy trust for client-IP resolution.
///
/// OFF by default: the unauthenticated token endpoint is rate-limited by the
/// trusted socket peer address only. Set to `1`/`true` ONLY when a trusted
/// reverse proxy terminates the connection, in which case `X-Forwarded-For` /
/// `X-Real-IP` is honored for the rate-limit identity (issue #48).
pub const ENV_TRUST_PROXY: &str = "AUTH_TRUST_PROXY";

/// Environment variable holding the comma-separated CORS allowlist of exact
/// origins (e.g. `http://localhost:5173,https://app.example.com`).
///
/// When set, it overrides the `[server] cors_allowed_origins` config field. When
/// neither is set, the server falls back to built-in local dev defaults
/// ([`DEFAULT_CORS_ALLOWED_ORIGINS`]) and never to a permissive wildcard.
pub const ENV_CORS_ALLOWED_ORIGINS: &str = "CORS_ALLOWED_ORIGINS";

/// Built-in, dev-friendly default CORS allowlist used when no origins are
/// configured via env or the config file. These are the common local frontend
/// dev origins; production deployments MUST set `CORS_ALLOWED_ORIGINS`.
pub const DEFAULT_CORS_ALLOWED_ORIGINS: [&str; 3] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "http://localhost:8080",
];

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
    /// Cleanup configuration.
    #[serde(default)]
    pub cleanup: CleanupConfig,
    /// Authentication configuration (JWT + x509). Optional in the file; env vars
    /// and built-in dev defaults fill any gaps (see [`AuthConfig::resolved`]).
    #[serde(default)]
    pub auth: Option<AuthConfig>,
    /// List of configured assets.
    pub assets: Vec<AssetConfig>,
}

/// Authentication configuration: PEM key/cert paths, issuer, and default TTL.
///
/// All fields fall back to built-in DEV defaults so local `cargo run` works out
/// of the box; production deployments override the paths via the `[auth]` section
/// or the `AUTH_*` environment variables. The operator bootstrap secret is NOT
/// stored here — it comes only from `AUTH_BOOTSTRAP_SECRET`.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// Path to the PEM-encoded RSA private signing key.
    #[serde(default = "default_private_key_path")]
    pub private_key_path: String,
    /// Path to the PEM-encoded x509 certificate (holds the public key).
    #[serde(default = "default_cert_path")]
    pub cert_path: String,
    /// Token issuer (the `iss` claim).
    #[serde(default = "default_issuer")]
    pub issuer: String,
    /// Default minted-token lifetime in seconds.
    #[serde(default = "default_token_ttl_secs")]
    pub default_ttl_secs: u64,
}

fn default_private_key_path() -> String {
    DEV_PRIVATE_KEY_PATH.to_string()
}

fn default_cert_path() -> String {
    DEV_CERT_PATH.to_string()
}

fn default_issuer() -> String {
    DEFAULT_ISSUER.to_string()
}

fn default_token_ttl_secs() -> u64 {
    DEFAULT_TOKEN_TTL_SECS
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            private_key_path: default_private_key_path(),
            cert_path: default_cert_path(),
            issuer: default_issuer(),
            default_ttl_secs: default_token_ttl_secs(),
        }
    }
}

impl AuthConfig {
    /// Resolves the effective auth configuration, layering (highest precedence
    /// first): `AUTH_*` environment variables, the `[auth]` config section, then
    /// built-in DEV defaults.
    #[must_use]
    pub fn resolved(config: Option<&Config>) -> Self {
        let base = config.and_then(|c| c.auth.clone()).unwrap_or_default();
        Self {
            private_key_path: std::env::var(ENV_PRIVATE_KEY_PATH).unwrap_or(base.private_key_path),
            cert_path: std::env::var(ENV_CERT_PATH).unwrap_or(base.cert_path),
            issuer: std::env::var(ENV_ISSUER).unwrap_or(base.issuer),
            default_ttl_secs: std::env::var(ENV_DEFAULT_TTL_SECS)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(base.default_ttl_secs),
        }
    }

    /// Returns true if this configuration points at the built-in DEV key pair.
    #[must_use]
    pub fn is_dev(&self) -> bool {
        self.private_key_path == DEV_PRIVATE_KEY_PATH && self.cert_path == DEV_CERT_PATH
    }

    /// Reads the operator bootstrap secret from `AUTH_BOOTSTRAP_SECRET`.
    ///
    /// Returns `None` when unset, which disables the token-issuance endpoint.
    #[must_use]
    pub fn bootstrap_secret() -> Option<String> {
        match std::env::var(ENV_BOOTSTRAP_SECRET) {
            Ok(s) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    /// Reads the reverse-proxy trust flag from `AUTH_TRUST_PROXY`.
    ///
    /// Returns `false` (the secure default) unless the variable is set to a
    /// truthy value (`1`, `true`, `yes`, `on`, case-insensitive). When `false`,
    /// the token endpoint rate-limits by the socket peer address and never trusts
    /// a client-supplied forwarding header.
    #[must_use]
    pub fn trust_proxy() -> bool {
        match std::env::var(ENV_TRUST_PROXY) {
            Ok(s) => matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            Err(_) => false,
        }
    }
}

/// Server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host address to bind to.
    pub host: String,
    /// Port number to listen on.
    pub port: u16,
    /// CORS allowlist of exact origins (`scheme://host[:port]`). When empty, the
    /// server falls back to built-in dev defaults unless `CORS_ALLOWED_ORIGINS`
    /// is set. The `CORS_ALLOWED_ORIGINS` environment variable (comma-separated)
    /// overrides this list. A permissive wildcard is never used.
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            cors_allowed_origins: Vec::new(),
        }
    }
}

/// Where the effective CORS allowlist was sourced from (for logging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorsOriginsSource {
    /// Resolved from the `CORS_ALLOWED_ORIGINS` environment variable.
    Env,
    /// Resolved from the `[server] cors_allowed_origins` config field.
    Config,
    /// Built-in local dev defaults (nothing explicitly configured).
    Default,
}

/// The resolved CORS allowlist together with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorsOrigins {
    /// Effective allowlist of origin strings (trimmed, non-empty).
    pub origins: Vec<String>,
    /// Where [`CorsOrigins::origins`] was sourced from.
    pub source: CorsOriginsSource,
}

/// Parses a comma-separated list of CORS origins.
///
/// Entries are split on `,`, trimmed of surrounding whitespace, and blank
/// entries are skipped. The returned strings are raw origins; conversion to a
/// validated HTTP header value (and skipping of invalid entries) happens at the
/// transport layer.
#[must_use]
pub fn parse_cors_origins_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Resolves the effective CORS allowlist from an explicit env value and the
/// config-file list, applying precedence (env > config > built-in defaults).
///
/// Factored out of [`resolved_cors_origins`] so the precedence logic is unit
/// testable without mutating process-wide environment variables.
#[must_use]
fn resolve_cors_origins_from(env: Option<&str>, config_origins: &[String]) -> CorsOrigins {
    if let Some(raw) = env {
        let parsed = parse_cors_origins_csv(raw);
        if !parsed.is_empty() {
            return CorsOrigins {
                origins: parsed,
                source: CorsOriginsSource::Env,
            };
        }
    }

    let from_config: Vec<String> = config_origins
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !from_config.is_empty() {
        return CorsOrigins {
            origins: from_config,
            source: CorsOriginsSource::Config,
        };
    }

    CorsOrigins {
        origins: DEFAULT_CORS_ALLOWED_ORIGINS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        source: CorsOriginsSource::Default,
    }
}

/// Resolves the effective CORS allowlist, layering (highest precedence first):
/// the `CORS_ALLOWED_ORIGINS` environment variable (comma-separated), the
/// `[server] cors_allowed_origins` config field, then the built-in dev defaults
/// ([`DEFAULT_CORS_ALLOWED_ORIGINS`]). Never falls back to a permissive wildcard.
#[must_use]
pub fn resolved_cors_origins(config: Option<&Config>) -> CorsOrigins {
    let env = std::env::var(ENV_CORS_ALLOWED_ORIGINS).ok();
    let config_origins: &[String] = config
        .map(|c| c.server.cors_allowed_origins.as_slice())
        .unwrap_or(&[]);
    resolve_cors_origins_from(env.as_deref(), config_origins)
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

/// Cleanup configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CleanupConfig {
    /// Interval in seconds between cleanup runs.
    pub interval_seconds: u64,
    /// Age in seconds after which filled/canceled orders are removed.
    pub retention_seconds: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 60,
            retention_seconds: 300, // 5 minutes
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
    /// Strikes are generated as `center + offset` for symmetric offsets and
    /// floored at `strike_spacing` so they stay strictly positive. Because that
    /// floor can collapse several low offsets onto the same value, consecutive
    /// duplicate strikes are skipped — the generation is monotonically
    /// non-decreasing, so the returned vector is strictly ascending and may
    /// therefore be shorter than `num_strikes`.
    ///
    /// # Returns
    /// Vector of distinct, ascending strike prices in cents.
    #[must_use]
    pub fn generate_strikes(&self) -> Vec<u64> {
        let center = self.initial_price;
        let half_count = self.num_strikes / 2;
        let mut strikes = Vec::with_capacity(self.num_strikes as usize);

        for i in 0..self.num_strikes {
            let offset = (i as f64 - half_count as f64) * self.strike_spacing;
            let strike = (center + offset).max(self.strike_spacing);
            // Convert to cents through the single canonical rounding helper. A
            // strike that is non-finite or out of range is logged and skipped
            // rather than truncated into a corrupt value (inputs are already
            // validated by `Config::validate`, so this is a defensive guard).
            match dollars_to_cents(strike) {
                // Skip a strike that collapsed onto the clamp floor (and so
                // duplicates the previous value): generation is non-decreasing,
                // so a duplicate can only equal the immediately preceding strike.
                Some(cents) if strikes.last() == Some(&cents) => {}
                Some(cents) => strikes.push(cents),
                None => tracing::warn!(
                    symbol = %self.symbol,
                    strike,
                    "skipping strike: dollar value is non-finite or out of range"
                ),
            }
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
        if let Some(auth) = &self.auth
            && auth.default_ttl_secs == 0
        {
            return Err(ConfigError::InvalidValue(
                "auth default_ttl_secs must be positive".to_string(),
            ));
        }

        // A zero interval makes `tokio::time::interval` panic in the simulation
        // ticker ("interval period must be non-zero"); reject it at load.
        if self.simulation.interval_ms == 0 {
            return Err(ConfigError::InvalidValue(
                "simulation interval_ms must be greater than zero".to_string(),
            ));
        }

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
            // Reject non-finite (NaN/±Inf) before any range comparison: IEEE-754
            // comparisons against NaN are always false, so a bare `<= 0.0` lets
            // NaN/Inf slip through into generate_strikes() and the pricer.
            if !asset.initial_price.is_finite() {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} initial_price must be finite, got {}",
                    asset.symbol, asset.initial_price
                )));
            }
            if asset.initial_price <= 0.0 {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} initial_price must be positive, got {}",
                    asset.symbol, asset.initial_price
                )));
            }
            if asset.initial_price > MAX_INITIAL_PRICE {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} initial_price {} exceeds maximum {}",
                    asset.symbol, asset.initial_price, MAX_INITIAL_PRICE
                )));
            }
            if !asset.volatility.is_finite() {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} volatility must be finite, got {}",
                    asset.symbol, asset.volatility
                )));
            }
            if asset.volatility <= 0.0 || asset.volatility > MAX_VOLATILITY {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} volatility must be between 0 and {}, got {}",
                    asset.symbol, MAX_VOLATILITY, asset.volatility
                )));
            }
            // Drift may be negative (a down-trending underlying) but never NaN/Inf.
            if !asset.drift.is_finite() {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} drift must be finite, got {}",
                    asset.symbol, asset.drift
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
            if !asset.strike_spacing.is_finite() {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} strike_spacing must be finite, got {}",
                    asset.symbol, asset.strike_spacing
                )));
            }
            if asset.strike_spacing <= 0.0 {
                return Err(ConfigError::InvalidValue(format!(
                    "asset {} strike_spacing must be positive, got {}",
                    asset.symbol, asset.strike_spacing
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
            cleanup: CleanupConfig::default(),
            auth: None,
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
    fn test_generate_strikes_dedups_clamp_floor_collapse() {
        // A center close to zero relative to the spacing forces several low
        // offsets to clamp onto the `strike_spacing` floor. Those collapsed
        // duplicates must be skipped so the result stays strictly ascending and
        // never emits the same strike twice.
        let asset = AssetConfig {
            symbol: "LOW".to_string(),
            name: "Low center".to_string(),
            initial_price: 15.0,
            volatility: 0.2,
            drift: 0.0,
            expirations: vec!["20251231".to_string()],
            num_strikes: 5,
            strike_spacing: 10.0,
        };

        // Raw offsets: -20,-10,0,10,20 -> 15+offset = -5,5,15,25,35 -> floored at
        // 10 -> 10,10,15,25,35. After dedup: 10,15,25,35 (in cents).
        let strikes = asset.generate_strikes();
        assert_eq!(strikes, vec![1000, 1500, 2500, 3500]);
        // Strictly ascending, no duplicates.
        assert!(strikes.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn test_generate_strikes_rounds_not_truncates() {
        // A strike at a `.999` dollar boundary must ROUND to the nearest cent
        // (10100), not truncate downward (10099) as the old `as u64` cast did.
        let asset = AssetConfig {
            symbol: "RND".to_string(),
            name: "Rounding".to_string(),
            initial_price: 100.999,
            volatility: 0.2,
            drift: 0.0,
            expirations: vec!["20251231".to_string()],
            num_strikes: 1,
            strike_spacing: 1.0,
        };

        let strikes = asset.generate_strikes();
        assert_eq!(strikes, vec![10100]);
    }

    #[test]
    fn test_dollars_to_cents_rounds_half_up() {
        // The exact issue acceptance cases: these FAIL under truncation (6 and
        // 10099) but pass under the single canonical `.round()` policy.
        assert_eq!(dollars_to_cents(0.07), Some(7));
        assert_eq!(dollars_to_cents(100.999), Some(10100));
    }

    #[test]
    fn test_dollars_to_cents_zero_and_basics() {
        assert_eq!(dollars_to_cents(0.0), Some(0));
        assert_eq!(dollars_to_cents(1.0), Some(100));
        assert_eq!(dollars_to_cents(150.50), Some(15050));
        // Half a cent rounds up; just under rounds down.
        assert_eq!(dollars_to_cents(0.005), Some(1));
        assert_eq!(dollars_to_cents(0.004), Some(0));
    }

    #[test]
    fn test_dollars_to_cents_rejects_invalid() {
        assert_eq!(dollars_to_cents(f64::NAN), None);
        assert_eq!(dollars_to_cents(f64::INFINITY), None);
        assert_eq!(dollars_to_cents(f64::NEG_INFINITY), None);
        assert_eq!(dollars_to_cents(-0.01), None);
        assert_eq!(dollars_to_cents(MAX_INITIAL_PRICE * 2.0), None);
    }

    #[test]
    fn test_dollars_to_cents_accepts_at_cap() {
        // The canonical cap itself is accepted and its cents fit in i64.
        let cents = dollars_to_cents(MAX_INITIAL_PRICE).expect("cap is in range");
        assert!(i64::try_from(cents).is_ok());
    }

    #[test]
    fn test_auth_config_defaults_are_dev() {
        let auth = AuthConfig::default();
        assert!(auth.is_dev());
        assert_eq!(auth.issuer, DEFAULT_ISSUER);
        assert_eq!(auth.default_ttl_secs, DEFAULT_TOKEN_TTL_SECS);
    }

    #[test]
    fn test_parse_config_with_auth_section() {
        let toml_content = r#"
[server]
host = "127.0.0.1"
port = 3000

[simulation]
enabled = true
interval_ms = 500
walk_type = "geometric_brownian"

[auth]
private_key_path = "/etc/ocob/key.pem"
cert_path = "/etc/ocob/cert.pem"
issuer = "prod-issuer"
default_ttl_secs = 900

[[assets]]
symbol = "BTC"
name = "Bitcoin"
initial_price = 100000.0
volatility = 0.65
drift = 0.05
expirations = ["20251231"]
num_strikes = 10
strike_spacing = 1000.0
"#;

        let config = Config::parse(toml_content).expect("should parse");
        let auth = config.auth.expect("auth section present");
        assert_eq!(auth.private_key_path, "/etc/ocob/key.pem");
        assert_eq!(auth.cert_path, "/etc/ocob/cert.pem");
        assert_eq!(auth.issuer, "prod-issuer");
        assert_eq!(auth.default_ttl_secs, 900);
        assert!(!auth.is_dev());
    }

    #[test]
    fn test_validation_rejects_zero_ttl() {
        let config = Config {
            server: ServerConfig::default(),
            simulation: SimulationConfig::default(),
            cleanup: CleanupConfig::default(),
            auth: Some(AuthConfig {
                default_ttl_secs: 0,
                ..AuthConfig::default()
            }),
            assets: vec![AssetConfig {
                symbol: "BTC".to_string(),
                name: "Bitcoin".to_string(),
                initial_price: 100.0,
                volatility: 0.2,
                drift: 0.0,
                expirations: vec!["20251231".to_string()],
                num_strikes: 2,
                strike_spacing: 10.0,
            }],
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_parse_cors_origins_csv_split_and_trim() {
        let parsed = parse_cors_origins_csv("http://a.com, http://b.com ,http://c.com");
        assert_eq!(
            parsed,
            vec![
                "http://a.com".to_string(),
                "http://b.com".to_string(),
                "http://c.com".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_cors_origins_csv_skips_blanks() {
        assert!(parse_cors_origins_csv("").is_empty());
        assert!(parse_cors_origins_csv("   ").is_empty());
        assert!(parse_cors_origins_csv(" , ,, ").is_empty());
        assert_eq!(
            parse_cors_origins_csv("  http://only.com  ,, "),
            vec!["http://only.com".to_string()]
        );
    }

    #[test]
    fn test_resolve_cors_origins_env_overrides_config() {
        let resolved = resolve_cors_origins_from(
            Some("http://env-a.com, http://env-b.com"),
            &["http://cfg.com".to_string()],
        );
        assert_eq!(resolved.source, CorsOriginsSource::Env);
        assert_eq!(
            resolved.origins,
            vec![
                "http://env-a.com".to_string(),
                "http://env-b.com".to_string()
            ]
        );
    }

    #[test]
    fn test_resolve_cors_origins_blank_env_falls_through_to_config() {
        let resolved = resolve_cors_origins_from(Some("  , , "), &["http://cfg.com".to_string()]);
        assert_eq!(resolved.source, CorsOriginsSource::Config);
        assert_eq!(resolved.origins, vec!["http://cfg.com".to_string()]);
    }

    #[test]
    fn test_resolve_cors_origins_defaults_when_unset() {
        let resolved = resolve_cors_origins_from(None, &[]);
        assert_eq!(resolved.source, CorsOriginsSource::Default);
        assert_eq!(resolved.origins.len(), DEFAULT_CORS_ALLOWED_ORIGINS.len());
        assert!(
            resolved
                .origins
                .iter()
                .any(|o| o == "http://localhost:5173")
        );
    }

    #[test]
    fn test_validation_empty_assets() {
        let config = Config {
            server: ServerConfig::default(),
            simulation: SimulationConfig::default(),
            cleanup: CleanupConfig::default(),
            auth: None,
            assets: vec![],
        };
        assert!(config.validate().is_err());
    }

    /// Builds a known-good asset for validation tests; mutate one field per case.
    fn valid_asset() -> AssetConfig {
        AssetConfig {
            symbol: "BTC".to_string(),
            name: "Bitcoin".to_string(),
            initial_price: 100.0,
            volatility: 0.2,
            drift: 0.0,
            expirations: vec!["20251231".to_string()],
            num_strikes: 4,
            strike_spacing: 10.0,
        }
    }

    /// Builds a known-good config wrapping a single asset.
    fn config_with(asset: AssetConfig) -> Config {
        Config {
            server: ServerConfig::default(),
            simulation: SimulationConfig::default(),
            cleanup: CleanupConfig::default(),
            auth: None,
            assets: vec![asset],
        }
    }

    /// Asserts validation fails and the message mentions the offending field.
    fn assert_invalid(config: &Config, needle: &str) {
        match config.validate() {
            Err(ConfigError::InvalidValue(msg)) => {
                assert!(
                    msg.contains(needle),
                    "expected error to mention {needle:?}, got: {msg}"
                );
            }
            Err(other) => panic!("expected InvalidValue, got {other:?}"),
            Ok(()) => panic!("expected validation to fail for {needle:?}"),
        }
    }

    #[test]
    fn test_validation_accepts_valid_config() {
        assert!(config_with(valid_asset()).validate().is_ok());
    }

    #[test]
    fn test_validation_accepts_negative_drift() {
        let asset = AssetConfig {
            drift: -0.25,
            ..valid_asset()
        };
        assert!(config_with(asset).validate().is_ok());
    }

    #[test]
    fn test_validation_rejects_nan_initial_price() {
        let asset = AssetConfig {
            initial_price: f64::NAN,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "initial_price");
    }

    #[test]
    fn test_validation_rejects_inf_initial_price() {
        let asset = AssetConfig {
            initial_price: f64::INFINITY,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "initial_price");
    }

    #[test]
    fn test_validation_rejects_negative_initial_price() {
        let asset = AssetConfig {
            initial_price: -1.0,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "initial_price");
    }

    #[test]
    fn test_validation_rejects_zero_initial_price() {
        let asset = AssetConfig {
            initial_price: 0.0,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "initial_price");
    }

    #[test]
    fn test_validation_rejects_over_cap_initial_price() {
        let asset = AssetConfig {
            initial_price: MAX_INITIAL_PRICE * 2.0,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "initial_price");
    }

    #[test]
    fn test_validation_accepts_at_cap_initial_price() {
        let asset = AssetConfig {
            initial_price: MAX_INITIAL_PRICE,
            ..valid_asset()
        };
        assert!(config_with(asset).validate().is_ok());
    }

    #[test]
    fn test_validation_rejects_nan_volatility() {
        let asset = AssetConfig {
            volatility: f64::NAN,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "volatility");
    }

    #[test]
    fn test_validation_rejects_inf_volatility() {
        let asset = AssetConfig {
            volatility: f64::INFINITY,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "volatility");
    }

    #[test]
    fn test_validation_rejects_nan_drift() {
        let asset = AssetConfig {
            drift: f64::NAN,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "drift");
    }

    #[test]
    fn test_validation_rejects_inf_drift() {
        let asset = AssetConfig {
            drift: f64::NEG_INFINITY,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "drift");
    }

    #[test]
    fn test_validation_rejects_nan_strike_spacing() {
        let asset = AssetConfig {
            strike_spacing: f64::NAN,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "strike_spacing");
    }

    #[test]
    fn test_validation_rejects_zero_strike_spacing() {
        let asset = AssetConfig {
            strike_spacing: 0.0,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "strike_spacing");
    }

    #[test]
    fn test_validation_rejects_negative_strike_spacing() {
        let asset = AssetConfig {
            strike_spacing: -5.0,
            ..valid_asset()
        };
        assert_invalid(&config_with(asset), "strike_spacing");
    }

    #[test]
    fn test_validation_rejects_zero_interval_ms() {
        let mut config = config_with(valid_asset());
        config.simulation.interval_ms = 0;
        assert_invalid(&config, "interval_ms");
    }
}

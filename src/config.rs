//! Configuration module for loading and parsing TOML configuration files.

use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

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
        if let Some(auth) = &self.auth
            && auth.default_ttl_secs == 0
        {
            return Err(ConfigError::InvalidValue(
                "auth default_ttl_secs must be positive".to_string(),
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
}

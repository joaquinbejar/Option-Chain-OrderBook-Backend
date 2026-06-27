//! Option Chain OrderBook Backend Server
//!
//! REST API server for interacting with the Option Chain OrderBook library.

use axum::http::Method;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use option_chain_orderbook_backend::api::create_router;
use option_chain_orderbook_backend::auth::JwtAuth;
use option_chain_orderbook_backend::config::{AuthConfig, Config};
use option_chain_orderbook_backend::db::DatabasePool;
use option_chain_orderbook_backend::models::Permission;
use option_chain_orderbook_backend::state::AppState;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use option_chain_orderbook_backend::api::controls::{
    InsertPriceResponse, InstrumentStatus, InstrumentToggleResponse, InstrumentsListResponse,
    KillSwitchResponse, LatestPriceResponse, SystemControlResponse, UpdateParametersResponse,
};
use option_chain_orderbook_backend::db::{InsertPriceRequest, UpdateParametersRequest};
use option_chain_orderbook_backend::models::{
    AddOrderRequest, AddOrderResponse, CancelOrderResponse, ExpirationSummary,
    ExpirationsListResponse, GlobalStatsResponse, HealthResponse, OrderBookSnapshotResponse,
    QuoteResponse, StrikeSummary, StrikesListResponse, TokenRequest, TokenResponse,
    UnderlyingSummary, UnderlyingsListResponse,
};

/// Builds tracing spans for HTTP requests recording the path but NEVER the query
/// string, so secrets such as the `?token=<jwt>` WebSocket upgrade parameter can
/// never leak into logs.
#[derive(Clone, Copy)]
struct RedactingMakeSpan;

impl<B> tower_http::trace::MakeSpan<B> for RedactingMakeSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        tracing::debug_span!(
            "http_request",
            method = %request.method(),
            path = %request.uri().path(),
            version = ?request.version(),
        )
    }
}

/// OpenAPI security scheme registration (HTTP bearer / JWT).
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

/// OpenAPI documentation.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        option_chain_orderbook_backend::api::handlers::health_check,
        option_chain_orderbook_backend::api::handlers::issue_token,
        option_chain_orderbook_backend::api::handlers::get_global_stats,
        option_chain_orderbook_backend::api::handlers::list_underlyings,
        option_chain_orderbook_backend::api::handlers::create_underlying,
        option_chain_orderbook_backend::api::handlers::get_underlying,
        option_chain_orderbook_backend::api::handlers::delete_underlying,
        option_chain_orderbook_backend::api::handlers::list_expirations,
        option_chain_orderbook_backend::api::handlers::create_expiration,
        option_chain_orderbook_backend::api::handlers::get_expiration,
        option_chain_orderbook_backend::api::handlers::list_strikes,
        option_chain_orderbook_backend::api::handlers::create_strike,
        option_chain_orderbook_backend::api::handlers::get_strike,
        option_chain_orderbook_backend::api::handlers::get_option_book,
        option_chain_orderbook_backend::api::handlers::add_order,
        option_chain_orderbook_backend::api::handlers::cancel_order,
        option_chain_orderbook_backend::api::handlers::get_option_quote,
        option_chain_orderbook_backend::api::controls::get_controls,
        option_chain_orderbook_backend::api::controls::kill_switch,
        option_chain_orderbook_backend::api::controls::enable_quoting,
        option_chain_orderbook_backend::api::controls::update_parameters,
        option_chain_orderbook_backend::api::controls::toggle_instrument,
        option_chain_orderbook_backend::api::controls::list_instruments,
        option_chain_orderbook_backend::api::controls::insert_price,
        option_chain_orderbook_backend::api::controls::get_latest_price,
        option_chain_orderbook_backend::api::controls::get_all_prices,
        option_chain_orderbook_backend::api::websocket::ws_handler,
    ),
    components(
        schemas(
            HealthResponse,
            TokenRequest,
            TokenResponse,
            GlobalStatsResponse,
            UnderlyingsListResponse,
            UnderlyingSummary,
            ExpirationsListResponse,
            ExpirationSummary,
            StrikesListResponse,
            StrikeSummary,
            OrderBookSnapshotResponse,
            QuoteResponse,
            AddOrderRequest,
            AddOrderResponse,
            CancelOrderResponse,
            SystemControlResponse,
            KillSwitchResponse,
            UpdateParametersResponse,
            UpdateParametersRequest,
            InstrumentToggleResponse,
            InstrumentsListResponse,
            InstrumentStatus,
            InsertPriceRequest,
            InsertPriceResponse,
            LatestPriceResponse,
        )
    ),
    tags(
        (name = "Health", description = "Health check endpoints"),
        (name = "Authentication", description = "JWT token issuance"),
        (name = "Statistics", description = "Global statistics"),
        (name = "Controls", description = "Market maker control endpoints"),
        (name = "Prices", description = "Underlying price management"),
        (name = "WebSocket", description = "Real-time WebSocket connection"),
        (name = "Underlyings", description = "Underlying asset management"),
        (name = "Expirations", description = "Expiration date management"),
        (name = "Strikes", description = "Strike price management"),
        (name = "Options", description = "Option order book management"),
    ),
    info(
        title = "Option Chain OrderBook API",
        version = "0.4.0",
        description = "REST API for managing option chain order books with market maker support",
        license(name = "MIT"),
        contact(name = "Joaquin Bejar", email = "jb@taunais.com")
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing. The default filter is plain `info` so request URIs are
    // not logged at debug — combined with `RedactingMakeSpan`, this prevents the
    // `?token=<jwt>` WebSocket query parameter from ever reaching the logs.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // CLI subcommand: `mint-token` signs a token offline and exits (no server).
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("mint-token") {
        return run_mint_token(&args);
    }

    // Load configuration
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = match Config::load(&config_path) {
        Ok(c) => {
            info!("Loaded configuration from {}", config_path);
            Some(c)
        }
        Err(e) => {
            warn!(
                "Failed to load config from {}: {}. Using defaults.",
                config_path, e
            );
            None
        }
    };

    // Try to connect to database if DATABASE_URL is set
    let db = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        info!("Connecting to database...");
        match DatabasePool::new(&database_url).await {
            Ok(db) => {
                // Run migrations
                if let Err(e) = db.run_migrations().await {
                    warn!("Failed to run migrations: {}", e);
                }
                info!("Database connected successfully");
                Some(db)
            }
            Err(e) => {
                warn!(
                    "Failed to connect to database: {}. Running without persistence.",
                    e
                );
                None
            }
        }
    } else {
        info!("DATABASE_URL not set, running without database persistence");
        None
    };

    // Load the JWT auth core (signing/verification keys) and bootstrap secret.
    let auth = load_jwt_auth(config.as_ref())?;
    let bootstrap_secret = AuthConfig::bootstrap_secret();
    if bootstrap_secret.is_some() {
        info!("Token issuance endpoint enabled (AUTH_BOOTSTRAP_SECRET set)");
    } else {
        warn!(
            "AUTH_BOOTSTRAP_SECRET not set; POST /api/v1/auth/token is disabled. Mint tokens with the `mint-token` CLI subcommand."
        );
    }

    // Create application state and inject the auth core.
    let mut app_state = match config {
        Some(cfg) => AppState::from_config(cfg, db),
        None => match db {
            Some(database) => AppState::with_database(database),
            None => AppState::new(),
        },
    };
    app_state.auth = auth;
    app_state.bootstrap_secret = bootstrap_secret;
    let state = Arc::new(app_state);

    // Start price simulation if enabled
    if let Some(ref simulator) = state.price_simulator {
        let sim = Arc::clone(simulator);
        let mm = Arc::clone(&state.market_maker);
        tokio::spawn(async move {
            sim.run(Some(mm)).await;
        });
        info!("Price simulation started");
    }

    // Start order cleanup task
    if let Some(ref config) = state.config {
        let state_clone = Arc::clone(&state);
        let interval_secs = config.cleanup.interval_seconds;
        let retention_secs = config.cleanup.retention_seconds;

        if interval_secs > 0 {
            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
                // Skip the first immediate tick
                interval.tick().await;

                loop {
                    interval.tick().await;
                    state_clone.cleanup_old_orders(retention_secs);
                }
            });
            info!(
                "Order cleanup task started (interval: {}s, retention: {}s)",
                interval_secs, retention_secs
            );
        }
    }

    // Get host and port for server binding
    let (host, port) = if let Some(ref cfg) = state.config {
        (cfg.server.host.clone(), cfg.server.port)
    } else {
        (
            std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            std::env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("PORT must be a valid number"),
        )
    };

    // Configure CORS deliberately: any origin (no credentials), explicit methods,
    // and the `Authorization` + `Content-Type` headers required for JWT auth.
    // Credentials are intentionally NOT enabled, so this is not a permissive
    // wildcard-with-credentials configuration.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    // Build the router
    let app = create_router(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(cors)
        .layer(TraceLayer::new_for_http().make_span_with(RedactingMakeSpan));

    // Start the server
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Loads the JWT auth core from the resolved auth configuration (env vars +
/// `[auth]` config section + built-in DEV defaults).
fn load_jwt_auth(config: Option<&Config>) -> anyhow::Result<Arc<JwtAuth>> {
    let auth_cfg = AuthConfig::resolved(config);
    if auth_cfg.is_dev() {
        // The committed dev key must never silently sign production tokens.
        let allow_dev_key = matches!(std::env::var("ALLOW_DEV_KEY").as_deref(), Ok("1"));
        if allow_dev_key {
            warn!(
                "ALLOW_DEV_KEY=1: signing with the built-in DEV key ({}). NOT for production.",
                auth_cfg.private_key_path
            );
        } else {
            error!(
                "Refusing to start: resolved auth key path is the built-in DEV fixture ({}). Set AUTH_PRIVATE_KEY_PATH and AUTH_CERT_PATH to production keys, or set ALLOW_DEV_KEY=1 to override for local development.",
                auth_cfg.private_key_path
            );
            anyhow::bail!("dev signing key not permitted without ALLOW_DEV_KEY=1");
        }
    }
    let auth = JwtAuth::from_paths(
        Path::new(&auth_cfg.private_key_path),
        Path::new(&auth_cfg.cert_path),
        auth_cfg.issuer.clone(),
        auth_cfg.default_ttl_secs,
    )
    .map_err(|e| anyhow::anyhow!("failed to load auth keys: {e}"))?;
    info!(issuer = %auth_cfg.issuer, "JWT auth core loaded");
    Ok(Arc::new(auth))
}

/// Parses a comma-separated permission list (e.g. `read,trade`).
fn parse_permissions(spec: &str) -> anyhow::Result<Vec<Permission>> {
    spec.split(',')
        .map(|p| match p.trim().to_lowercase().as_str() {
            "read" => Ok(Permission::Read),
            "trade" => Ok(Permission::Trade),
            "admin" => Ok(Permission::Admin),
            other => Err(anyhow::anyhow!("unknown permission: {other}")),
        })
        .collect()
}

/// Runs the `mint-token` CLI subcommand: signs a JWT offline using the private
/// key and writes it to stdout, without starting the server.
///
/// Usage: `mint-token [--permissions read,trade,admin] [--ttl <seconds>]`.
fn run_mint_token(args: &[String]) -> anyhow::Result<()> {
    let mut permissions_arg: Option<String> = None;
    let mut ttl_arg: Option<u64> = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--permissions" | "-p" => {
                i += 1;
                permissions_arg = args.get(i).cloned();
            }
            "--ttl" | "-t" => {
                i += 1;
                ttl_arg = match args.get(i) {
                    Some(v) => Some(
                        v.parse()
                            .map_err(|_| anyhow::anyhow!("invalid --ttl value: {v}"))?,
                    ),
                    None => return Err(anyhow::anyhow!("--ttl requires a value")),
                };
            }
            other => return Err(anyhow::anyhow!("unknown argument: {other}")),
        }
        i += 1;
    }

    let permissions = parse_permissions(permissions_arg.as_deref().unwrap_or("read"))?;

    // Load config only to resolve the auth key/cert paths.
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = Config::load(&config_path).ok();
    let auth = load_jwt_auth(config.as_ref())?;

    let ttl_secs = ttl_arg.unwrap_or_else(|| auth.default_ttl_secs());
    let (token, _exp) = auth
        .mint_token(permissions, ttl_secs)
        .map_err(|e| anyhow::anyhow!("failed to mint token: {e}"))?;

    // The minted token is this command's primary output (intended provisioning
    // output, not logging) — write it to stdout.
    let mut stdout = std::io::stdout();
    writeln!(stdout, "{token}")?;
    Ok(())
}

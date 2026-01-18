//! Option Chain OrderBook Backend Server
//!
//! REST API server for interacting with the Option Chain OrderBook library.

use option_chain_orderbook_backend::api::create_router;
use option_chain_orderbook_backend::config::Config;
use option_chain_orderbook_backend::db::DatabasePool;
use option_chain_orderbook_backend::state::AppState;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
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
    ExpirationsListResponse, GlobalStatsResponse, HealthResponse, LastTradeResponse,
    OrderBookSnapshotResponse, QuoteResponse, StrikeSummary, StrikesListResponse,
    UnderlyingSummary, UnderlyingsListResponse,
};

/// OpenAPI documentation.
#[derive(OpenApi)]
#[openapi(
    paths(
        option_chain_orderbook_backend::api::handlers::health_check,
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
        option_chain_orderbook_backend::api::handlers::get_last_trade,
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
            LastTradeResponse,
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
        version = "0.1.0",
        description = "REST API for managing option chain order books with market maker support",
        license(name = "MIT"),
        contact(name = "Joaquin Bejar", email = "jb@taunais.com")
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

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

    // Create application state
    let state = if let Some(cfg) = config {
        let host = cfg.server.host.clone();
        let port = cfg.server.port;
        let state = Arc::new(AppState::from_config(cfg, db));

        // Start price simulation if enabled
        if let Some(ref simulator) = state.price_simulator {
            let sim = Arc::clone(simulator);
            let mm = Arc::clone(&state.market_maker);
            tokio::spawn(async move {
                sim.run(Some(mm)).await;
            });
            info!("Price simulation started");
        }

        info!(
            "Starting Option Chain OrderBook Backend on {}:{}",
            host, port
        );
        info!(
            "Swagger UI available at http://{}:{}/swagger-ui/",
            host, port
        );
        info!("WebSocket available at ws://{}:{}/ws", host, port);

        state
    } else {
        let state = match db {
            Some(database) => Arc::new(AppState::with_database(database)),
            None => Arc::new(AppState::new()),
        };

        // Get host and port from environment or use defaults
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .expect("PORT must be a valid number");

        info!(
            "Starting Option Chain OrderBook Backend on {}:{}",
            host, port
        );
        info!(
            "Swagger UI available at http://{}:{}/swagger-ui/",
            host, port
        );
        info!("WebSocket available at ws://{}:{}/ws", host, port);

        state
    };

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

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the router
    let app = create_router(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Start the server
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

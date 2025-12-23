//! Option Chain OrderBook Backend Server
//!
//! REST API server for interacting with the Option Chain OrderBook library.

use option_chain_orderbook_backend::api::create_router;
use option_chain_orderbook_backend::state::AppState;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use option_chain_orderbook_backend::models::{
    AddOrderRequest, AddOrderResponse, CancelOrderResponse, ExpirationSummary,
    ExpirationsListResponse, GlobalStatsResponse, HealthResponse, OrderBookSnapshotResponse,
    QuoteResponse, StrikeSummary, StrikesListResponse, UnderlyingSummary, UnderlyingsListResponse,
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
        )
    ),
    tags(
        (name = "Health", description = "Health check endpoints"),
        (name = "Statistics", description = "Global statistics"),
        (name = "Underlyings", description = "Underlying asset management"),
        (name = "Expirations", description = "Expiration date management"),
        (name = "Strikes", description = "Strike price management"),
        (name = "Options", description = "Option order book management"),
    ),
    info(
        title = "Option Chain OrderBook API",
        version = "0.1.0",
        description = "REST API for managing option chain order books",
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

    // Create application state
    let state = Arc::new(AppState::new());

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

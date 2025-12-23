//! Route configuration.

use crate::api::{controls, handlers, websocket};
use crate::state::AppState;
use axum::Router;
use axum::routing::{delete, get, post};
use std::sync::Arc;

/// Creates the API router.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // WebSocket
        .route("/ws", get(websocket::ws_handler))
        // Statistics
        .route("/api/v1/stats", get(handlers::get_global_stats))
        // Controls
        .route("/api/v1/controls", get(controls::get_controls))
        .route("/api/v1/controls/kill-switch", post(controls::kill_switch))
        .route("/api/v1/controls/enable", post(controls::enable_quoting))
        .route("/api/v1/controls/parameters", post(controls::update_parameters))
        .route("/api/v1/controls/instruments", get(controls::list_instruments))
        .route(
            "/api/v1/controls/instrument/{symbol}/toggle",
            post(controls::toggle_instrument),
        )
        // Prices
        .route("/api/v1/prices", get(controls::get_all_prices).post(controls::insert_price))
        .route("/api/v1/prices/{symbol}", get(controls::get_latest_price))
        // Underlyings
        .route("/api/v1/underlyings", get(handlers::list_underlyings))
        .route(
            "/api/v1/underlyings/{underlying}",
            post(handlers::create_underlying)
                .get(handlers::get_underlying)
                .delete(handlers::delete_underlying),
        )
        // Expirations
        .route(
            "/api/v1/underlyings/{underlying}/expirations",
            get(handlers::list_expirations),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}",
            post(handlers::create_expiration).get(handlers::get_expiration),
        )
        // Strikes
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes",
            get(handlers::list_strikes),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}",
            post(handlers::create_strike).get(handlers::get_strike),
        )
        // Options
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}",
            get(handlers::get_option_book),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders",
            post(handlers::add_order),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders/{order_id}",
            delete(handlers::cancel_order),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/quote",
            get(handlers::get_option_quote),
        )
        .with_state(state)
}

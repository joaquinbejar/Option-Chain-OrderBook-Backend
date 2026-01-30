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
        // Authentication
        .route(
            "/api/v1/auth/keys",
            post(handlers::create_api_key).get(handlers::list_api_keys),
        )
        .route(
            "/api/v1/auth/keys/{key_id}",
            delete(handlers::delete_api_key),
        )
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
        // Volatility Surface
        .route(
            "/api/v1/underlyings/{underlying}/volatility-surface",
            get(handlers::get_volatility_surface),
        )
        // Option Chain Matrix
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/chain",
            get(handlers::get_option_chain),
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
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders/market",
            post(handlers::submit_market_order),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/orders/{order_id}",
            delete(handlers::cancel_order).patch(handlers::modify_order),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/quote",
            get(handlers::get_option_quote),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/greeks",
            get(handlers::get_option_greeks),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/snapshot",
            get(handlers::get_option_snapshot),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/last-trade",
            get(handlers::get_last_trade),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/ohlc",
            get(handlers::get_ohlc),
        )
        .route(
            "/api/v1/underlyings/{underlying}/expirations/{expiration}/strikes/{strike}/options/{style}/metrics",
            get(handlers::get_orderbook_metrics),
        )
        // Order status and query
        .route("/api/v1/orders", get(handlers::list_orders))
        .route("/api/v1/orders/{order_id}", get(handlers::get_order_status))
        // Bulk order operations
        .route(
            "/api/v1/orders/bulk",
            post(handlers::bulk_submit_orders).delete(handlers::bulk_cancel_orders),
        )
        .route(
            "/api/v1/orders/cancel-all",
            delete(handlers::cancel_all_orders),
        )
        // Position tracking
        .route("/api/v1/positions", get(handlers::list_positions))
        .route("/api/v1/positions/{symbol}", get(handlers::get_position))
        .with_state(state)
}

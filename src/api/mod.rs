//! API routes module.

pub mod controls;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod websocket;

pub use middleware::rate_limit_middleware;
pub use routes::create_router;

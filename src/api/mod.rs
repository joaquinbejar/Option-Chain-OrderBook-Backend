//! API routes module.

pub mod controls;
pub mod handlers;
pub mod routes;
pub mod websocket;

pub use routes::create_router;

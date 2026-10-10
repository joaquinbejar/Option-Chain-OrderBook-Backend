//! Order identifier generation.

use orderbook_rs::OrderId;

/// Returns a fresh, randomly generated order identifier.
///
/// The identifier is a ULID, so ids sort by creation time and keep the
/// 26-character wire format clients already see. `pricelevel` 0.10 removed
/// the infallible `Id::new()` this replaces; it generated the same ULID.
#[must_use]
pub fn new_order_id() -> OrderId {
    OrderId::Ulid(ulid::Ulid::generate())
}

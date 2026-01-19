//! In-memory order tracking for order lifecycle management.
//!
//! High-performance order storage using DashMap for lock-free concurrency:
//! - Order creation and tracking
//! - Fill history recording
//! - Order status updates
//! - Filtered queries with pagination
//! - Periodic cleanup of old filled/canceled orders

use crate::models::{
    OrderFillRecord, OrderInfo, OrderListQuery, OrderListResponse, OrderSide, OrderStatus,
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, info};

/// Configuration for order cleanup.
#[derive(Clone)]
pub struct CleanupConfig {
    /// How often to run cleanup (in seconds).
    pub interval_secs: u64,
    /// Age threshold for filled/canceled orders to be removed (in seconds).
    pub max_age_secs: i64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 5 minutes
            max_age_secs: 3600, // 1 hour
        }
    }
}

/// In-memory order tracker with DashMap for high-performance concurrent access.
pub struct OrderTracker {
    orders: Arc<DashMap<String, OrderInfo>>,
    cleanup_handle: Option<JoinHandle<()>>,
}

impl Default for OrderTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderTracker {
    /// Creates a new in-memory order tracker.
    pub fn new() -> Self {
        Self {
            orders: Arc::new(DashMap::new()),
            cleanup_handle: None,
        }
    }

    /// Creates an order tracker with automatic cleanup.
    pub fn with_cleanup(config: CleanupConfig) -> Self {
        let orders = Arc::new(DashMap::new());
        let orders_clone = Arc::clone(&orders);

        // Spawn cleanup task
        let handle = tokio::spawn(async move {
            Self::cleanup_loop(orders_clone, config).await;
        });

        Self {
            orders,
            cleanup_handle: Some(handle),
        }
    }

    /// Background cleanup loop that periodically removes old filled/canceled orders.
    async fn cleanup_loop(orders: Arc<DashMap<String, OrderInfo>>, config: CleanupConfig) {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(config.interval_secs));

        loop {
            interval.tick().await;

            let threshold = Utc::now() - Duration::seconds(config.max_age_secs);
            let mut removed_count = 0;

            // Collect keys to remove (can't remove while iterating in DashMap)
            let keys_to_remove: Vec<String> = orders
                .iter()
                .filter_map(|entry| {
                    let order = entry.value();
                    // Only clean up filled or canceled orders
                    if order.status == OrderStatus::Filled || order.status == OrderStatus::Canceled
                    {
                        // Parse updated_at timestamp
                        if let Ok(updated) = DateTime::parse_from_rfc3339(&order.updated_at) {
                            if updated.with_timezone(&Utc) < threshold {
                                return Some(entry.key().clone());
                            }
                        }
                    }
                    None
                })
                .collect();

            // Remove old orders
            for key in keys_to_remove {
                orders.remove(&key);
                removed_count += 1;
            }

            if removed_count > 0 {
                info!("Cleaned up {} old filled/canceled orders", removed_count);
            }
            debug!("Order cleanup complete. Active orders: {}", orders.len());
        }
    }

    /// Constructs the option symbol from components.
    /// Format: "{underlying}-{expiration}-{strike}-{C|P}"
    pub fn build_symbol(
        underlying: &str,
        expiration: &str,
        strike: u64,
        option_style: &str,
    ) -> String {
        let style_char = if option_style.to_lowercase() == "call" {
            "C"
        } else {
            "P"
        };
        format!("{}-{}-{}-{}", underlying, expiration, strike, style_char)
    }

    /// Creates a new order in the tracker.
    pub fn create_order(
        &self,
        order_id: &str,
        underlying: &str,
        expiration: &str,
        strike: u64,
        option_style: &str,
        side: OrderSide,
        price: u64,
        quantity: u64,
    ) {
        let symbol = Self::build_symbol(underlying, expiration, strike, option_style);
        let now = Utc::now().to_rfc3339();

        let order = OrderInfo {
            order_id: order_id.to_string(),
            symbol,
            side,
            price,
            original_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            status: OrderStatus::Active,
            time_in_force: "GTC".to_string(),
            created_at: now.clone(),
            updated_at: now,
            fills: Vec::new(),
        };

        self.orders.insert(order_id.to_string(), order);
    }

    /// Gets order information by order ID.
    pub fn get_order(&self, order_id: &str) -> Option<OrderInfo> {
        self.orders.get(order_id).map(|entry| entry.value().clone())
    }

    /// Lists orders with optional filters and pagination.
    pub fn list_orders(&self, query: &OrderListQuery) -> OrderListResponse {
        // Filter orders
        let filtered: Vec<OrderInfo> = self
            .orders
            .iter()
            .filter_map(|entry| {
                let order = entry.value();

                // Filter by underlying (extract from symbol)
                if let Some(ref underlying) = query.underlying {
                    if !order.symbol.starts_with(underlying) {
                        return None;
                    }
                }

                // Filter by status
                if let Some(ref status) = query.status {
                    if order.status.to_string() != status.to_lowercase() {
                        return None;
                    }
                }

                // Filter by side
                if let Some(ref side) = query.side {
                    if order.side.to_string() != side.to_lowercase() {
                        return None;
                    }
                }

                Some(order.clone())
            })
            .collect();

        let total = filtered.len();

        // Apply pagination
        let paginated: Vec<OrderInfo> = filtered
            .into_iter()
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect();

        OrderListResponse {
            orders: paginated,
            total,
            limit: query.limit,
            offset: query.offset,
        }
    }

    /// Records a fill for an order.
    pub fn record_fill(&self, order_id: &str, price: u64, quantity: u64) {
        if let Some(mut entry) = self.orders.get_mut(order_id) {
            let order = entry.value_mut();

            let fill = OrderFillRecord {
                price,
                quantity,
                timestamp: Utc::now().to_rfc3339(),
            };
            order.fills.push(fill);
            order.filled_quantity += quantity;
            order.remaining_quantity = order.remaining_quantity.saturating_sub(quantity);
            order.updated_at = Utc::now().to_rfc3339();

            // Update status
            if order.remaining_quantity == 0 {
                order.status = OrderStatus::Filled;
            } else if order.filled_quantity > 0 {
                order.status = OrderStatus::Partial;
            }
        }
    }

    /// Cancels an order.
    pub fn cancel_order(&self, order_id: &str) -> bool {
        if let Some(mut entry) = self.orders.get_mut(order_id) {
            let order = entry.value_mut();
            if order.status == OrderStatus::Active || order.status == OrderStatus::Partial {
                order.status = OrderStatus::Canceled;
                order.updated_at = Utc::now().to_rfc3339();
                return true;
            }
        }
        false
    }

    /// Returns the number of tracked orders (for monitoring).
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    /// Manually triggers cleanup of old orders (for testing).
    pub fn cleanup_old_orders(&self, max_age_secs: i64) -> usize {
        let threshold = Utc::now() - Duration::seconds(max_age_secs);
        let mut removed_count = 0;

        let keys_to_remove: Vec<String> = self
            .orders
            .iter()
            .filter_map(|entry| {
                let order = entry.value();
                if order.status == OrderStatus::Filled || order.status == OrderStatus::Canceled {
                    if let Ok(updated) = DateTime::parse_from_rfc3339(&order.updated_at) {
                        if updated.with_timezone(&Utc) < threshold {
                            return Some(entry.key().clone());
                        }
                    }
                }
                None
            })
            .collect();

        for key in keys_to_remove {
            self.orders.remove(&key);
            removed_count += 1;
        }

        removed_count
    }
}

impl Drop for OrderTracker {
    fn drop(&mut self) {
        // Abort cleanup task if it exists
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::OrderSide;

    fn create_tracker() -> OrderTracker {
        OrderTracker::new()
    }

    #[test]
    fn test_create_and_get_order() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-123",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );

        let order = tracker.get_order("order-123").expect("Order should exist");

        assert_eq!(order.order_id, "order-123");
        assert_eq!(order.symbol, "BTC-20251231-100000-C");
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.price, 5000);
        assert_eq!(order.original_quantity, 10);
        assert_eq!(order.remaining_quantity, 10);
        assert_eq!(order.filled_quantity, 0);
        assert_eq!(order.status, OrderStatus::Active);
        assert!(order.fills.is_empty());
    }

    #[test]
    fn test_get_nonexistent_order() {
        let tracker = create_tracker();
        assert!(tracker.get_order("nonexistent").is_none());
    }

    #[test]
    fn test_build_symbol() {
        assert_eq!(
            OrderTracker::build_symbol("BTC", "20251231", 100000, "call"),
            "BTC-20251231-100000-C"
        );
        assert_eq!(
            OrderTracker::build_symbol("ETH", "20260115", 5000, "put"),
            "ETH-20260115-5000-P"
        );
        assert_eq!(
            OrderTracker::build_symbol("BTC", "20251231", 100000, "CALL"),
            "BTC-20251231-100000-C"
        );
    }

    #[test]
    fn test_list_orders_no_filter() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        tracker.create_order(
            "order-2",
            "ETH",
            "20251231",
            5000,
            "put",
            OrderSide::Sell,
            300,
            5,
        );

        let query = OrderListQuery {
            underlying: None,
            status: None,
            side: None,
            limit: 100,
            offset: 0,
        };

        let response = tracker.list_orders(&query);
        assert_eq!(response.total, 2);
        assert_eq!(response.orders.len(), 2);
    }

    #[test]
    fn test_list_orders_filter_by_underlying() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        tracker.create_order(
            "order-2",
            "ETH",
            "20251231",
            5000,
            "put",
            OrderSide::Sell,
            300,
            5,
        );
        tracker.create_order(
            "order-3",
            "BTC",
            "20251231",
            110000,
            "call",
            OrderSide::Buy,
            4000,
            10,
        );

        let query = OrderListQuery {
            underlying: Some("BTC".to_string()),
            status: None,
            side: None,
            limit: 100,
            offset: 0,
        };

        let response = tracker.list_orders(&query);
        assert_eq!(response.total, 2);
        assert!(response.orders.iter().all(|o| o.symbol.starts_with("BTC")));
    }

    #[test]
    fn test_list_orders_filter_by_side() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        tracker.create_order(
            "order-2",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Sell,
            5100,
            5,
        );

        let query = OrderListQuery {
            underlying: None,
            status: None,
            side: Some("buy".to_string()),
            limit: 100,
            offset: 0,
        };

        let response = tracker.list_orders(&query);
        assert_eq!(response.total, 1);
        assert_eq!(response.orders[0].side, OrderSide::Buy);
    }

    #[test]
    fn test_list_orders_pagination() {
        let tracker = create_tracker();

        for i in 0..10 {
            tracker.create_order(
                &format!("order-{}", i),
                "BTC",
                "20251231",
                100000,
                "call",
                OrderSide::Buy,
                5000 + i,
                10,
            );
        }

        // First page
        let query = OrderListQuery {
            underlying: None,
            status: None,
            side: None,
            limit: 3,
            offset: 0,
        };
        let response = tracker.list_orders(&query);
        assert_eq!(response.total, 10);
        assert_eq!(response.orders.len(), 3);
        assert_eq!(response.limit, 3);
        assert_eq!(response.offset, 0);

        // Second page
        let query = OrderListQuery {
            underlying: None,
            status: None,
            side: None,
            limit: 3,
            offset: 3,
        };
        let response = tracker.list_orders(&query);
        assert_eq!(response.total, 10);
        assert_eq!(response.orders.len(), 3);
        assert_eq!(response.offset, 3);
    }

    #[test]
    fn test_record_fill_partial() {
        let tracker = create_tracker();
        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );

        tracker.record_fill("order-1", 5000, 3);

        let order = tracker.get_order("order-1").unwrap();
        assert_eq!(order.filled_quantity, 3);
        assert_eq!(order.remaining_quantity, 7);
        assert_eq!(order.status, OrderStatus::Partial);
        assert_eq!(order.fills.len(), 1);
        assert_eq!(order.fills[0].price, 5000);
        assert_eq!(order.fills[0].quantity, 3);
    }

    #[test]
    fn test_record_fill_complete() {
        let tracker = create_tracker();
        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );

        tracker.record_fill("order-1", 5000, 10);

        let order = tracker.get_order("order-1").unwrap();
        assert_eq!(order.filled_quantity, 10);
        assert_eq!(order.remaining_quantity, 0);
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_record_fill_multiple() {
        let tracker = create_tracker();
        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );

        tracker.record_fill("order-1", 5000, 3);
        tracker.record_fill("order-1", 5010, 4);
        tracker.record_fill("order-1", 5005, 3);

        let order = tracker.get_order("order-1").unwrap();
        assert_eq!(order.filled_quantity, 10);
        assert_eq!(order.remaining_quantity, 0);
        assert_eq!(order.status, OrderStatus::Filled);
        assert_eq!(order.fills.len(), 3);
    }

    #[test]
    fn test_cancel_order() {
        let tracker = create_tracker();
        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );

        let result = tracker.cancel_order("order-1");
        assert!(result);

        let order = tracker.get_order("order-1").unwrap();
        assert_eq!(order.status, OrderStatus::Canceled);
    }

    #[test]
    fn test_cancel_already_filled_order() {
        let tracker = create_tracker();
        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        tracker.record_fill("order-1", 5000, 10); // Fully fill

        let result = tracker.cancel_order("order-1");
        assert!(!result); // Cannot cancel filled order

        let order = tracker.get_order("order-1").unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let tracker = create_tracker();
        let result = tracker.cancel_order("nonexistent");
        assert!(!result);
    }

    #[test]
    fn test_order_count() {
        let tracker = create_tracker();
        assert_eq!(tracker.order_count(), 0);

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        assert_eq!(tracker.order_count(), 1);

        tracker.create_order(
            "order-2",
            "ETH",
            "20251231",
            5000,
            "put",
            OrderSide::Sell,
            300,
            5,
        );
        assert_eq!(tracker.order_count(), 2);
    }

    #[test]
    fn test_list_orders_filter_by_status() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        tracker.create_order(
            "order-2",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5100,
            10,
        );
        tracker.record_fill("order-2", 5100, 10); // Mark as filled

        let query = OrderListQuery {
            underlying: None,
            status: Some("active".to_string()),
            side: None,
            limit: 100,
            offset: 0,
        };

        let response = tracker.list_orders(&query);
        assert_eq!(response.total, 1);
        assert_eq!(response.orders[0].order_id, "order-1");
    }

    #[test]
    fn test_cleanup_removes_old_filled_orders() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );
        tracker.create_order(
            "order-2",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5100,
            10,
        );

        // Fill one order
        tracker.record_fill("order-2", 5100, 10);

        // Cleanup with 0 second threshold removes all filled orders
        // (threshold is Utc::now() - 0 seconds = now, so all filled orders are older)
        let removed = tracker.cleanup_old_orders(0);
        assert_eq!(removed, 1); // The filled order should be removed

        // Active orders should never be cleaned
        assert!(tracker.get_order("order-1").is_some());
        // Filled order was cleaned up
        assert!(tracker.get_order("order-2").is_none());
    }

    #[test]
    fn test_cleanup_does_not_remove_active_orders() {
        let tracker = create_tracker();

        tracker.create_order(
            "order-1",
            "BTC",
            "20251231",
            100000,
            "call",
            OrderSide::Buy,
            5000,
            10,
        );

        // Even with 0 age threshold, active orders should not be removed
        let removed = tracker.cleanup_old_orders(0);
        assert_eq!(removed, 0);
        assert!(tracker.get_order("order-1").is_some());
    }
}

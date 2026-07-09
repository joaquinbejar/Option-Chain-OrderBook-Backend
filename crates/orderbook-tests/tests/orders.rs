//! Order lifecycle tests: status/list, modify, bulk submit/cancel, cancel-all.
//!
//! Placement uses [`TEST_EXPIRATION`]; modify / single-cancel / bulk-write / read
//! paths use the server-formatted expiration (see the crate docs for bug #110).
//!
//! Every test creates an underlying, so each uses the capture-then-assert
//! pattern: perform all requests capturing outcomes into plain variables, run
//! [`cleanup_underlying`], and only THEN assert — a failed assertion never leaks
//! a test underlying.

use orderbook_client::{
    AddOrderRequest, BulkCancelRequest, BulkOrderItem, BulkOrderRequest, BulkOrderStatus,
    CancelAllQuery, Error, ModifyOrderRequest, ModifyOrderStatus, OptionPath, OptionStyle,
    OrderListQuery, OrderSide, OrderStatus,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, setup_underlying,
};

/// Places a resting buy and returns its order id (or the placement error). The
/// caller captures the `Result` and only unwraps in the assert phase, so a
/// placement failure never skips cleanup.
async fn place_buy(
    client: &orderbook_client::OrderbookClient,
    underlying: &str,
    price: u128,
    quantity: u64,
) -> Result<String, Error> {
    let place = OptionPath::call(underlying, TEST_EXPIRATION, TEST_STRIKE);
    Ok(client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price,
                quantity,
            },
        )
        .await?
        .order_id)
}

#[tokio::test]
async fn test_order_status_and_list() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _fmt) = setup_underlying(&client, "OST").await;

    // Phase 1: place, read status (depends on the id), list filtered.
    let order_id = place_buy(&client, &underlying, 1400, 10).await;
    let status = match order_id.as_ref().ok() {
        Some(id) => Some(client.get_order_status(id).await),
        None => None,
    };
    let listed = client
        .list_orders(Some(&OrderListQuery {
            underlying: Some(underlying.clone()),
            ..Default::default()
        }))
        .await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    let order_id = order_id.expect("add order");

    // Single order status (flat response shape).
    let status = status
        .expect("status should have run")
        .expect("get order status");
    assert_eq!(status.order_id, order_id);
    assert!(status.symbol.contains(&underlying));
    assert_eq!(status.side, OrderSide::Buy);
    assert_eq!(status.price, 1400);
    assert_eq!(status.original_quantity, 10);
    assert_eq!(status.remaining_quantity, 10);
    assert_eq!(status.filled_quantity, 0);
    assert_eq!(status.status, OrderStatus::Active);

    // Listing filtered to this underlying returns the order.
    let listed = listed.expect("list orders");
    assert_eq!(listed.total, 1);
    assert_eq!(listed.orders.len(), 1);
    assert_eq!(listed.orders[0].order_id, order_id);
}

#[tokio::test]
async fn test_modify_order() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "MOD").await;

    // Phase 1: place, modify (depends on the id, resolves the book by the
    // formatted expiration), then read the book.
    let order_id = place_buy(&client, &underlying, 1400, 10).await;
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);
    let modified = match order_id.as_ref().ok() {
        Some(id) => Some(
            client
                .modify_order(
                    &read,
                    id,
                    &ModifyOrderRequest {
                        price: Some(1450),
                        quantity: Some(12),
                    },
                )
                .await,
        ),
        None => None,
    };
    let book = client.get_option_book(&read).await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    let order_id = order_id.expect("add order");
    let modified = modified
        .expect("modify should have run")
        .expect("modify order");
    assert_eq!(modified.status, ModifyOrderStatus::Modified);
    assert_eq!(modified.new_price, Some(1450));
    assert_eq!(modified.new_quantity, Some(12));
    assert!(modified.priority_changed);
    // Cancel-and-replace assigns a new order id.
    assert_ne!(modified.order_id, order_id);

    // The book reflects the modified resting order.
    let book = book.expect("book");
    assert_eq!(book.quote.bid_price, Some(1450));
    assert_eq!(book.quote.bid_size, 12);
}

#[tokio::test]
async fn test_bulk_submit_partial() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "BSP").await;

    // Bulk submit resolves each book by the formatted expiration; one good order
    // and one against a non-existent strike. Non-atomic: the good one survives.
    let response = client
        .bulk_submit_orders(&BulkOrderRequest {
            orders: vec![
                BulkOrderItem {
                    underlying: underlying.clone(),
                    expiration: formatted.clone(),
                    strike: TEST_STRIKE,
                    style: OptionStyle::Call,
                    side: OrderSide::Buy,
                    price: 1300,
                    quantity: 5,
                },
                BulkOrderItem {
                    underlying: underlying.clone(),
                    expiration: formatted.clone(),
                    strike: 99_999_999, // never created
                    style: OptionStyle::Call,
                    side: OrderSide::Buy,
                    price: 10,
                    quantity: 1,
                },
            ],
            atomic: false,
        })
        .await;

    cleanup_underlying(&client, &underlying).await;

    let response = response.expect("bulk submit");
    assert_eq!(response.success_count, 1);
    assert_eq!(response.failure_count, 1);
    assert!(!response.rolled_back);
    assert_eq!(response.results.len(), 2);
    let accepted = &response.results[0];
    assert_eq!(accepted.status, BulkOrderStatus::Accepted);
    assert!(accepted.order_id.is_some());
    let rejected = &response.results[1];
    assert_eq!(rejected.status, BulkOrderStatus::Rejected);
    assert!(rejected.error.is_some());
}

#[tokio::test]
async fn test_bulk_submit_atomic_rollback() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "BSA").await;

    // Atomic: the second order fails, so the first is rolled back and nothing is
    // left resting.
    let response = client
        .bulk_submit_orders(&BulkOrderRequest {
            orders: vec![
                BulkOrderItem {
                    underlying: underlying.clone(),
                    expiration: formatted.clone(),
                    strike: TEST_STRIKE,
                    style: OptionStyle::Call,
                    side: OrderSide::Buy,
                    price: 1300,
                    quantity: 2,
                },
                BulkOrderItem {
                    underlying: underlying.clone(),
                    expiration: formatted.clone(),
                    strike: 99_999_999,
                    style: OptionStyle::Call,
                    side: OrderSide::Buy,
                    price: 10,
                    quantity: 1,
                },
            ],
            atomic: true,
        })
        .await;

    // No order survived: the book has no resting bid.
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);
    let book = client.get_option_book(&read).await;

    cleanup_underlying(&client, &underlying).await;

    let response = response.expect("bulk submit atomic");
    assert!(response.rolled_back);
    assert_eq!(response.success_count, 0);
    assert_eq!(response.failure_count, 2);

    let book = book.expect("book");
    assert_eq!(book.order_count, 0);
}

#[tokio::test]
async fn test_bulk_cancel() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "BCN").await;

    // Phase 1: place two orders, then bulk-cancel them (plus an unknown id).
    let oid1 = place_buy(&client, &underlying, 1300, 4).await;
    let oid2 = place_buy(&client, &underlying, 1350, 6).await;
    let response = match (oid1.as_ref().ok(), oid2.as_ref().ok()) {
        (Some(id1), Some(id2)) => Some(
            client
                .bulk_cancel_orders(&BulkCancelRequest {
                    order_ids: vec![id1.clone(), id2.clone(), "not-a-real-order".to_string()],
                })
                .await,
        ),
        _ => None,
    };

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    let oid1 = oid1.expect("place first order");
    let oid2 = oid2.expect("place second order");
    let response = response
        .expect("both orders should have been placed")
        .expect("bulk cancel");
    assert_eq!(response.success_count, 2);
    assert_eq!(response.failure_count, 1);
    let canceled: Vec<&String> = response
        .results
        .iter()
        .filter(|r| r.canceled)
        .map(|r| &r.order_id)
        .collect();
    assert!(canceled.contains(&&oid1));
    assert!(canceled.contains(&&oid2));
}

#[tokio::test]
async fn test_cancel_all() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "CAL").await;

    // Phase 1: two resting orders that do not cross, then cancel-all filtered to
    // this underlying.
    let buy = place_buy(&client, &underlying, 1300, 4).await;
    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    let sell = client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1700,
                quantity: 3,
            },
        )
        .await;
    let response = client
        .cancel_all_orders(Some(&CancelAllQuery {
            underlying: Some(underlying.clone()),
            ..Default::default()
        }))
        .await;

    // Phase 2: cleanup.
    cleanup_underlying(&client, &underlying).await;

    // Phase 3: assert.
    buy.expect("place resting buy");
    sell.expect("add resting sell");
    let response = response.expect("cancel all");
    assert_eq!(response.canceled_count, 2);
    assert_eq!(response.failed_count, 0);
}

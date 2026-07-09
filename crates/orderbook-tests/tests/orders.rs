//! Order lifecycle tests: status/list, modify, bulk submit/cancel, cancel-all.
//!
//! Placement uses [`TEST_EXPIRATION`]; modify / single-cancel / bulk-write / read
//! paths use the server-formatted expiration (see the crate docs for bug #110).

use orderbook_client::{
    AddOrderRequest, BulkCancelRequest, BulkOrderItem, BulkOrderRequest, BulkOrderStatus,
    CancelAllQuery, ModifyOrderRequest, ModifyOrderStatus, OptionPath, OrderListQuery, OrderSide,
    OrderStatus,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, setup_underlying,
};

async fn place_buy(
    client: &orderbook_client::OrderbookClient,
    underlying: &str,
    price: u128,
    quantity: u64,
) -> String {
    let place = OptionPath::call(underlying, TEST_EXPIRATION, TEST_STRIKE);
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Buy,
                price,
                quantity,
            },
        )
        .await
        .expect("add order")
        .order_id
}

#[tokio::test]
async fn test_order_status_and_list() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _fmt) = setup_underlying(&client, "OST").await;

    let order_id = place_buy(&client, &underlying, 1400, 10).await;

    // Single order status (flat response shape).
    let status = client
        .get_order_status(&order_id)
        .await
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
    let listed = client
        .list_orders(Some(&OrderListQuery {
            underlying: Some(underlying.clone()),
            ..Default::default()
        }))
        .await
        .expect("list orders");
    assert_eq!(listed.total, 1);
    assert_eq!(listed.orders.len(), 1);
    assert_eq!(listed.orders[0].order_id, order_id);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_modify_order() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "MOD").await;

    let order_id = place_buy(&client, &underlying, 1400, 10).await;

    // Modify resolves the book by the formatted expiration.
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);
    let modified = client
        .modify_order(
            &read,
            &order_id,
            &ModifyOrderRequest {
                price: Some(1450),
                quantity: Some(12),
            },
        )
        .await
        .expect("modify order");

    assert_eq!(modified.status, ModifyOrderStatus::Modified);
    assert_eq!(modified.new_price, Some(1450));
    assert_eq!(modified.new_quantity, Some(12));
    assert!(modified.priority_changed);
    // Cancel-and-replace assigns a new order id.
    assert_ne!(modified.order_id, order_id);

    // The book reflects the modified resting order.
    let book = client.get_option_book(&read).await.expect("book");
    assert_eq!(book.quote.bid_price, Some(1450));
    assert_eq!(book.quote.bid_size, 12);

    cleanup_underlying(&client, &underlying).await;
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
                    style: "call".to_string(),
                    side: OrderSide::Buy,
                    price: 1300,
                    quantity: 5,
                },
                BulkOrderItem {
                    underlying: underlying.clone(),
                    expiration: formatted.clone(),
                    strike: 99_999_999, // never created
                    style: "call".to_string(),
                    side: OrderSide::Buy,
                    price: 10,
                    quantity: 1,
                },
            ],
            atomic: false,
        })
        .await
        .expect("bulk submit");

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

    cleanup_underlying(&client, &underlying).await;
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
                    style: "call".to_string(),
                    side: OrderSide::Buy,
                    price: 1300,
                    quantity: 2,
                },
                BulkOrderItem {
                    underlying: underlying.clone(),
                    expiration: formatted.clone(),
                    strike: 99_999_999,
                    style: "call".to_string(),
                    side: OrderSide::Buy,
                    price: 10,
                    quantity: 1,
                },
            ],
            atomic: true,
        })
        .await
        .expect("bulk submit atomic");

    assert!(response.rolled_back);
    assert_eq!(response.success_count, 0);
    assert_eq!(response.failure_count, 2);

    // No order survived: the book has no resting bid.
    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);
    let book = client.get_option_book(&read).await.expect("book");
    assert_eq!(book.order_count, 0);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_bulk_cancel() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "BCN").await;

    let oid1 = place_buy(&client, &underlying, 1300, 4).await;
    let oid2 = place_buy(&client, &underlying, 1350, 6).await;

    let response = client
        .bulk_cancel_orders(&BulkCancelRequest {
            order_ids: vec![oid1.clone(), oid2.clone(), "not-a-real-order".to_string()],
        })
        .await
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

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_cancel_all() {
    let client = admin_client().await.expect("admin client");
    let (underlying, _formatted) = setup_underlying(&client, "CAL").await;

    // Two resting orders that do not cross.
    place_buy(&client, &underlying, 1300, 4).await;
    let place = OptionPath::call(&underlying, TEST_EXPIRATION, TEST_STRIKE);
    client
        .add_order(
            &place,
            &AddOrderRequest {
                side: OrderSide::Sell,
                price: 1700,
                quantity: 3,
            },
        )
        .await
        .expect("add resting sell");

    let response = client
        .cancel_all_orders(Some(&CancelAllQuery {
            underlying: Some(underlying.clone()),
            ..Default::default()
        }))
        .await
        .expect("cancel all");

    assert_eq!(response.canceled_count, 2);
    assert_eq!(response.failed_count, 0);

    cleanup_underlying(&client, &underlying).await;
}

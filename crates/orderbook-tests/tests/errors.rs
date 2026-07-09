//! Negative-path tests asserting the SPECIFIC typed error for each documented
//! failure: the three 404 resolution variants, unknown-order 404s, 400 for
//! malformed input and out-of-range control parameters, plus 401/403 authz.
//!
//! Authz basics on the stats/controls endpoints are covered in `auth.rs`; the
//! 401/403 cases here extend that to the order-placement (Trade) path.

use orderbook_client::{
    AddOrderRequest, Error, ModifyOrderRequest, OptionPath, OrderSide, UpdateParametersRequest,
};
use orderbook_tests::{
    TEST_EXPIRATION, TEST_STRIKE, admin_client, cleanup_underlying, create_test_client,
    read_client, setup_underlying, unique_symbol,
};

/// A syntactically valid but never-issued order id (all-zero ULID).
const ABSENT_ORDER_ID: &str = "00000000000000000000000000";

fn assert_status(result: Result<impl std::fmt::Debug, Error>, expected: u16) {
    match result {
        Err(Error::Api { status, .. }) => assert_eq!(status, expected, "unexpected status"),
        other => panic!("expected Api status {expected}, got {other:?}"),
    }
}

fn assert_not_found(result: Result<impl std::fmt::Debug, Error>) {
    match result {
        Err(Error::NotFound(_)) => {}
        other => panic!("expected NotFound (404), got {other:?}"),
    }
}

#[tokio::test]
async fn test_unknown_underlying_is_not_found() {
    let client = read_client().await.expect("read client");
    let path = OptionPath::call("GHOST_UNDERLYING_XYZ", TEST_EXPIRATION, TEST_STRIKE);
    assert_not_found(client.get_option_book(&path).await);
}

#[tokio::test]
async fn test_unknown_expiration_and_strike_are_not_found() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "ERX").await;

    // Existing underlying, expiration that resolves to no book.
    let bad_exp = OptionPath::call(&underlying, "77777777", TEST_STRIKE);
    assert_not_found(client.get_option_book(&bad_exp).await);

    // Existing underlying + expiration, missing strike.
    let bad_strike = OptionPath::call(&underlying, &formatted, 99_999_999);
    assert_not_found(client.get_option_book(&bad_strike).await);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_unknown_order_status_is_not_found() {
    let client = read_client().await.expect("read client");
    assert_not_found(client.get_order_status("no-such-order-id").await);
}

#[tokio::test]
async fn test_modify_unknown_order_is_not_found() {
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "ERM").await;

    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);
    let result = client
        .modify_order(
            &read,
            ABSENT_ORDER_ID,
            &ModifyOrderRequest {
                price: Some(1234),
                quantity: None,
            },
        )
        .await;
    assert_not_found(result);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_cancel_unknown_order_reports_failure() {
    // Canceling a valid-format but absent order on an existing book is not an
    // error: the server responds 200 with `success = false` (idempotent delete).
    let client = admin_client().await.expect("admin client");
    let (underlying, formatted) = setup_underlying(&client, "ERC").await;

    let read = OptionPath::call(&underlying, &formatted, TEST_STRIKE);
    let response = client
        .cancel_order(&read, ABSENT_ORDER_ID)
        .await
        .expect("cancel returns a body");
    assert!(!response.success);

    cleanup_underlying(&client, &underlying).await;
}

#[tokio::test]
async fn test_malformed_expiration_is_bad_request() {
    let client = admin_client().await.expect("admin client");
    let symbol = unique_symbol("ERB");

    // Non-numeric, non-date expiration.
    let banana = OptionPath::call(&symbol, "banana", TEST_STRIKE);
    assert_status(
        client
            .add_order(
                &banana,
                &AddOrderRequest {
                    side: OrderSide::Buy,
                    price: 100,
                    quantity: 1,
                },
            )
            .await,
        400,
    );

    // Non-positive expiration.
    let negative = OptionPath::call(&symbol, "-5", TEST_STRIKE);
    assert_status(
        client
            .add_order(
                &negative,
                &AddOrderRequest {
                    side: OrderSide::Buy,
                    price: 100,
                    quantity: 1,
                },
            )
            .await,
        400,
    );

    // Parsing fails before any underlying is created, but clean up defensively.
    cleanup_underlying(&client, &symbol).await;
}

#[tokio::test]
async fn test_out_of_range_control_parameters_are_bad_request() {
    // These are validated before any state is applied, so they leave the global
    // configuration untouched (no restore needed). NaN is not JSON-serializable,
    // so it is not exercised here.
    let client = admin_client().await.expect("admin client");

    assert_status(
        client
            .update_parameters(&UpdateParametersRequest {
                spread_multiplier: None,
                size_scalar: Some(1.5), // fraction must be in [0.0, 1.0]
                directional_skew: None,
            })
            .await,
        400,
    );

    assert_status(
        client
            .update_parameters(&UpdateParametersRequest {
                spread_multiplier: Some(100.0), // must be in [0.1, 10.0]
                size_scalar: None,
                directional_skew: None,
            })
            .await,
        400,
    );

    assert_status(
        client
            .update_parameters(&UpdateParametersRequest {
                spread_multiplier: None,
                size_scalar: None,
                directional_skew: Some(2.0), // must be in [-1.0, 1.0]
            })
            .await,
        400,
    );
}

#[tokio::test]
async fn test_unauthenticated_order_is_unauthorized() {
    // No token on a mutating endpoint -> 401 (extends auth.rs, which uses stats).
    let client = create_test_client().expect("client builds");
    let path = OptionPath::call("BTC", TEST_EXPIRATION, TEST_STRIKE);
    assert_status(
        client
            .add_order(
                &path,
                &AddOrderRequest {
                    side: OrderSide::Buy,
                    price: 100,
                    quantity: 1,
                },
            )
            .await,
        401,
    );
}

#[tokio::test]
async fn test_read_token_cannot_place_order() {
    // Read token on a Trade endpoint -> 403 (extends auth.rs, which uses controls).
    let client = read_client().await.expect("read client");
    let path = OptionPath::call("BTC", TEST_EXPIRATION, TEST_STRIKE);
    assert_status(
        client
            .add_order(
                &path,
                &AddOrderRequest {
                    side: OrderSide::Buy,
                    price: 100,
                    quantity: 1,
                },
            )
            .await,
        403,
    );
}

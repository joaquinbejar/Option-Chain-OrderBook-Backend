//! Unit tests for types module.

use super::*;

// ============================================================================
// OrderSide Tests
// ============================================================================

#[test]
fn test_order_side_display_buy() {
    let side = OrderSide::Buy;
    assert_eq!(format!("{}", side), "buy");
}

#[test]
fn test_order_side_display_sell() {
    let side = OrderSide::Sell;
    assert_eq!(format!("{}", side), "sell");
}

#[test]
fn test_order_side_serialization() {
    let buy = OrderSide::Buy;
    let sell = OrderSide::Sell;

    assert_eq!(serde_json::to_string(&buy).unwrap(), "\"buy\"");
    assert_eq!(serde_json::to_string(&sell).unwrap(), "\"sell\"");
}

#[test]
fn test_order_side_deserialization() {
    let buy: OrderSide = serde_json::from_str("\"buy\"").unwrap();
    let sell: OrderSide = serde_json::from_str("\"sell\"").unwrap();

    assert_eq!(buy, OrderSide::Buy);
    assert_eq!(sell, OrderSide::Sell);
}

// ============================================================================
// MarketOrderStatus Tests
// ============================================================================

#[test]
fn test_market_order_status_display_filled() {
    let status = MarketOrderStatus::Filled;
    assert_eq!(format!("{}", status), "filled");
}

#[test]
fn test_market_order_status_display_partial() {
    let status = MarketOrderStatus::Partial;
    assert_eq!(format!("{}", status), "partial");
}

#[test]
fn test_market_order_status_display_rejected() {
    let status = MarketOrderStatus::Rejected;
    assert_eq!(format!("{}", status), "rejected");
}

#[test]
fn test_market_order_status_serialization() {
    assert_eq!(
        serde_json::to_string(&MarketOrderStatus::Filled).unwrap(),
        "\"filled\""
    );
    assert_eq!(
        serde_json::to_string(&MarketOrderStatus::Partial).unwrap(),
        "\"partial\""
    );
    assert_eq!(
        serde_json::to_string(&MarketOrderStatus::Rejected).unwrap(),
        "\"rejected\""
    );
}

#[test]
fn test_market_order_status_deserialization() {
    let filled: MarketOrderStatus = serde_json::from_str("\"filled\"").unwrap();
    let partial: MarketOrderStatus = serde_json::from_str("\"partial\"").unwrap();
    let rejected: MarketOrderStatus = serde_json::from_str("\"rejected\"").unwrap();

    assert_eq!(filled, MarketOrderStatus::Filled);
    assert_eq!(partial, MarketOrderStatus::Partial);
    assert_eq!(rejected, MarketOrderStatus::Rejected);
}

// ============================================================================
// HealthResponse Tests
// ============================================================================

#[test]
fn test_health_response_serialization() {
    let response = HealthResponse {
        status: "ok".to_string(),
        version: "1.0.0".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"status\":\"ok\""));
    assert!(json.contains("\"version\":\"1.0.0\""));
}

#[test]
fn test_health_response_deserialization() {
    let json = r#"{"status":"healthy","version":"2.0.0"}"#;
    let response: HealthResponse = serde_json::from_str(json).unwrap();

    assert_eq!(response.status, "healthy");
    assert_eq!(response.version, "2.0.0");
}

// ============================================================================
// GlobalStatsResponse Tests
// ============================================================================

#[test]
fn test_global_stats_response_serialization() {
    let response = GlobalStatsResponse {
        underlying_count: 10,
        total_expirations: 50,
        total_strikes: 500,
        total_orders: 1000,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"underlying_count\":10"));
    assert!(json.contains("\"total_expirations\":50"));
    assert!(json.contains("\"total_strikes\":500"));
    assert!(json.contains("\"total_orders\":1000"));
}

// ============================================================================
// OptionPath Tests
// ============================================================================

#[test]
fn test_option_path_new() {
    let path = OptionPath::new("AAPL", "20240315", 15000, "call");

    assert_eq!(path.underlying, "AAPL");
    assert_eq!(path.expiration, "20240315");
    assert_eq!(path.strike, 15000);
    assert_eq!(path.style, "call");
}

#[test]
fn test_option_path_call() {
    let path = OptionPath::call("SPY", "20240329", 50000);

    assert_eq!(path.underlying, "SPY");
    assert_eq!(path.expiration, "20240329");
    assert_eq!(path.strike, 50000);
    assert_eq!(path.style, "call");
}

#[test]
fn test_option_path_put() {
    let path = OptionPath::put("QQQ", "20240412", 40000);

    assert_eq!(path.underlying, "QQQ");
    assert_eq!(path.expiration, "20240412");
    assert_eq!(path.strike, 40000);
    assert_eq!(path.style, "put");
}

// ============================================================================
// AddOrderRequest Tests
// ============================================================================

#[test]
fn test_add_order_request_serialization() {
    let request = AddOrderRequest {
        side: OrderSide::Buy,
        price: 10000,
        quantity: 100,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"side\":\"buy\""));
    assert!(json.contains("\"price\":10000"));
    assert!(json.contains("\"quantity\":100"));
}

#[test]
fn test_add_order_request_deserialization() {
    let json = r#"{"side":"sell","price":15000,"quantity":50}"#;
    let request: AddOrderRequest = serde_json::from_str(json).unwrap();

    assert_eq!(request.side, OrderSide::Sell);
    assert_eq!(request.price, 15000);
    assert_eq!(request.quantity, 50);
}

// ============================================================================
// MarketOrderRequest Tests
// ============================================================================

#[test]
fn test_market_order_request_serialization() {
    let request = MarketOrderRequest {
        side: OrderSide::Buy,
        quantity: 200,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"side\":\"buy\""));
    assert!(json.contains("\"quantity\":200"));
}

// ============================================================================
// MarketOrderResponse Tests
// ============================================================================

#[test]
fn test_market_order_response_serialization() {
    let response = MarketOrderResponse {
        order_id: "order-123".to_string(),
        status: MarketOrderStatus::Filled,
        filled_quantity: 100,
        remaining_quantity: 0,
        average_price: Some(150.50),
        fills: vec![FillInfo {
            price: 15050,
            quantity: 100,
        }],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"order_id\":\"order-123\""));
    assert!(json.contains("\"status\":\"filled\""));
    assert!(json.contains("\"filled_quantity\":100"));
}

// ============================================================================
// QuoteResponse Tests
// ============================================================================

#[test]
fn test_quote_response_serialization() {
    let response = QuoteResponse {
        bid_price: Some(10000),
        bid_size: 100,
        ask_price: Some(10100),
        ask_size: 150,
        timestamp_ms: 1704067200000,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"bid_price\":10000"));
    assert!(json.contains("\"ask_price\":10100"));
}

#[test]
fn test_quote_response_with_none_prices() {
    let response = QuoteResponse {
        bid_price: None,
        bid_size: 0,
        ask_price: None,
        ask_size: 0,
        timestamp_ms: 1704067200000,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"bid_price\":null"));
    assert!(json.contains("\"ask_price\":null"));
}

// ============================================================================
// EnrichedSnapshotResponse Tests
// ============================================================================

#[test]
fn test_enriched_snapshot_response_serialization() {
    let response = EnrichedSnapshotResponse {
        symbol: "AAPL-20240315-150C".to_string(),
        sequence: 12345,
        timestamp_ms: 1704067200000,
        bids: vec![PriceLevelInfo {
            price: 10000,
            quantity: 100,
            order_count: 5,
        }],
        asks: vec![PriceLevelInfo {
            price: 10100,
            quantity: 150,
            order_count: 3,
        }],
        stats: SnapshotStats {
            mid_price: Some(10050.0),
            spread_bps: Some(100.0),
            bid_depth_total: 100,
            ask_depth_total: 150,
            imbalance: -0.2,
            vwap_bid: Some(10000.0),
            vwap_ask: Some(10100.0),
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"symbol\":\"AAPL-20240315-150C\""));
    assert!(json.contains("\"sequence\":12345"));
}

// ============================================================================
// SystemControlResponse Tests
// ============================================================================

#[test]
fn test_system_control_response_serialization() {
    let response = SystemControlResponse {
        master_enabled: true,
        spread_multiplier: 1.5,
        size_scalar: 2.0,
        directional_skew: 0.1,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"master_enabled\":true"));
    assert!(json.contains("\"spread_multiplier\":1.5"));
}

// ============================================================================
// UpdateParametersRequest Tests
// ============================================================================

#[test]
fn test_update_parameters_request_serialization() {
    let request = UpdateParametersRequest {
        spread_multiplier: Some(1.5),
        size_scalar: Some(2.0),
        directional_skew: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"spreadMultiplier\":1.5"));
    assert!(json.contains("\"sizeScalar\":2.0"));
    assert!(!json.contains("directionalSkew"));
}

// ============================================================================
// InsertPriceRequest Tests
// ============================================================================

#[test]
fn test_insert_price_request_serialization() {
    let request = InsertPriceRequest {
        symbol: "AAPL".to_string(),
        price: 150.50,
        bid: Some(150.40),
        ask: Some(150.60),
        volume: Some(1000000),
        source: Some("exchange".to_string()),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"symbol\":\"AAPL\""));
    assert!(json.contains("\"price\":150.5"));
}

#[test]
fn test_insert_price_request_minimal() {
    let request = InsertPriceRequest {
        symbol: "SPY".to_string(),
        price: 450.0,
        bid: None,
        ask: None,
        volume: None,
        source: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"symbol\":\"SPY\""));
    assert!(!json.contains("\"bid\""));
    assert!(!json.contains("\"ask\""));
}

// ============================================================================
// Permission Tests
// ============================================================================

#[test]
fn test_permission_serialization() {
    assert_eq!(
        serde_json::to_string(&Permission::Read).unwrap(),
        "\"read\""
    );
    assert_eq!(
        serde_json::to_string(&Permission::Trade).unwrap(),
        "\"trade\""
    );
    assert_eq!(
        serde_json::to_string(&Permission::Admin).unwrap(),
        "\"admin\""
    );
}

#[test]
fn test_permission_deserialization() {
    let read: Permission = serde_json::from_str("\"read\"").unwrap();
    let trade: Permission = serde_json::from_str("\"trade\"").unwrap();
    let admin: Permission = serde_json::from_str("\"admin\"").unwrap();

    assert_eq!(read, Permission::Read);
    assert_eq!(trade, Permission::Trade);
    assert_eq!(admin, Permission::Admin);
}

// ============================================================================
// CreateApiKeyRequest Tests
// ============================================================================

#[test]
fn test_create_api_key_request_serialization() {
    let request = CreateApiKeyRequest {
        name: "test-key".to_string(),
        permissions: vec![Permission::Read, Permission::Trade],
        rate_limit: 500,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"name\":\"test-key\""));
    assert!(json.contains("\"permissions\":[\"read\",\"trade\"]"));
    assert!(json.contains("\"rate_limit\":500"));
}

// ============================================================================
// ExecutionInfo Tests
// ============================================================================

#[test]
fn test_execution_info_serialization() {
    let info = ExecutionInfo {
        execution_id: "exec-123".to_string(),
        order_id: "order-456".to_string(),
        symbol: "AAPL-20240315-150C".to_string(),
        side: OrderSide::Buy,
        price: 10000,
        quantity: 100,
        timestamp_ms: 1704067200000,
        counterparty_order_id: Some("order-789".to_string()),
        is_maker: true,
        fee: 10,
        edge: Some(50),
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"execution_id\":\"exec-123\""));
    assert!(json.contains("\"is_maker\":true"));
}

// ============================================================================
// PositionInfo Tests
// ============================================================================

#[test]
fn test_position_info_serialization() {
    let info = PositionInfo {
        symbol: "AAPL-20240315-150C".to_string(),
        quantity: 100,
        average_price: 10000,
        realized_pnl: 500,
        updated_at: 1704067200000,
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"symbol\":\"AAPL-20240315-150C\""));
    assert!(json.contains("\"quantity\":100"));
}

// ============================================================================
// OrderbookSnapshotInfo Tests
// ============================================================================

#[test]
fn test_orderbook_snapshot_info_serialization() {
    let info = OrderbookSnapshotInfo {
        snapshot_id: "snap-123".to_string(),
        underlying: "AAPL".to_string(),
        expiration: "20240315".to_string(),
        strike: 15000,
        style: "call".to_string(),
        order_count: 10,
        bid_levels: 5,
        ask_levels: 5,
        data: "{}".to_string(),
        created_at: 1704067200000,
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"snapshot_id\":\"snap-123\""));
    assert!(json.contains("\"underlying\":\"AAPL\""));
}

// ============================================================================
// OrderStatus Tests
// ============================================================================

#[test]
fn test_order_status_serialization() {
    assert_eq!(
        serde_json::to_string(&OrderStatus::Active).unwrap(),
        "\"active\""
    );
    assert_eq!(
        serde_json::to_string(&OrderStatus::Filled).unwrap(),
        "\"filled\""
    );
    assert_eq!(
        serde_json::to_string(&OrderStatus::Canceled).unwrap(),
        "\"canceled\""
    );
    assert_eq!(
        serde_json::to_string(&OrderStatus::Expired).unwrap(),
        "\"expired\""
    );
}

// ============================================================================
// ModifyOrderRequest Tests
// ============================================================================

#[test]
fn test_modify_order_request_serialization() {
    let request = ModifyOrderRequest {
        price: Some(15000),
        quantity: Some(200),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"price\":15000"));
    assert!(json.contains("\"quantity\":200"));
}

#[test]
fn test_modify_order_request_partial() {
    let request = ModifyOrderRequest {
        price: Some(15000),
        quantity: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"price\":15000"));
    assert!(!json.contains("\"quantity\""));
}

// ============================================================================
// BulkOrderRequest Tests
// ============================================================================

#[test]
fn test_bulk_order_request_serialization() {
    let request = BulkOrderRequest {
        orders: vec![
            BulkOrderItem {
                symbol: "AAPL-20240315-150C".to_string(),
                side: OrderSide::Buy,
                price: 10000,
                quantity: 100,
            },
            BulkOrderItem {
                symbol: "AAPL-20240315-150P".to_string(),
                side: OrderSide::Sell,
                price: 5000,
                quantity: 50,
            },
        ],
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"orders\":["));
    assert!(json.contains("\"symbol\":\"AAPL-20240315-150C\""));
}

// ============================================================================
// GreeksData Tests
// ============================================================================

#[test]
fn test_greeks_data_serialization() {
    let greeks = GreeksData {
        delta: 0.5,
        gamma: 0.02,
        theta: -0.05,
        vega: 0.15,
        rho: 0.01,
    };

    let json = serde_json::to_string(&greeks).unwrap();
    assert!(json.contains("\"delta\":0.5"));
    assert!(json.contains("\"gamma\":0.02"));
    assert!(json.contains("\"theta\":-0.05"));
}

// ============================================================================
// OhlcBar Tests
// ============================================================================

#[test]
fn test_ohlc_bar_serialization() {
    let bar = OhlcBar {
        timestamp_ms: 1704067200000,
        open: 10000,
        high: 10500,
        low: 9800,
        close: 10200,
        volume: 50000,
        trade_count: 100,
    };

    let json = serde_json::to_string(&bar).unwrap();
    assert!(json.contains("\"open\":10000"));
    assert!(json.contains("\"high\":10500"));
    assert!(json.contains("\"low\":9800"));
    assert!(json.contains("\"close\":10200"));
}

// ============================================================================
// OhlcQuery Tests
// ============================================================================

#[test]
fn test_ohlc_query_default() {
    let query = OhlcQuery::default();

    assert!(query.interval.is_none());
    assert!(query.from.is_none());
    assert!(query.to.is_none());
    assert!(query.limit.is_none());
}

// ============================================================================
// SpreadMetrics Tests
// ============================================================================

#[test]
fn test_spread_metrics_serialization() {
    let metrics = SpreadMetrics {
        spread_absolute: Some(100),
        spread_bps: Some(50.0),
        spread_percent: Some(0.5),
    };

    let json = serde_json::to_string(&metrics).unwrap();
    assert!(json.contains("\"spread_absolute\":100"));
    assert!(json.contains("\"spread_bps\":50.0"));
}

// ============================================================================
// VolatilitySurfaceResponse Tests
// ============================================================================

#[test]
fn test_volatility_surface_response_serialization() {
    let mut surface = std::collections::HashMap::new();
    let mut strike_ivs = std::collections::HashMap::new();
    strike_ivs.insert(
        15000u64,
        StrikeIV {
            call_iv: Some(0.25),
            put_iv: Some(0.27),
        },
    );
    surface.insert("20240315".to_string(), strike_ivs);

    let response = VolatilitySurfaceResponse {
        underlying: "AAPL".to_string(),
        underlying_price: Some(150.0),
        expirations: vec!["20240315".to_string()],
        strikes: vec![15000],
        surface,
        timestamp_ms: 1704067200000,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"underlying\":\"AAPL\""));
}

// ============================================================================
// OptionChainResponse Tests
// ============================================================================

#[test]
fn test_option_chain_response_serialization() {
    let response = OptionChainResponse {
        underlying: "AAPL".to_string(),
        expiration: "20240315".to_string(),
        underlying_price: Some(150.0),
        chain: vec![ChainStrikeRow {
            strike: 15000,
            call: OptionQuoteData {
                bid: Some(1000),
                ask: Some(1100),
                bid_size: 100,
                ask_size: 150,
                last: Some(1050),
                volume: 5000,
                open_interest: 10000,
                iv: Some(0.25),
                delta: Some(0.5),
            },
            put: OptionQuoteData {
                bid: Some(500),
                ask: Some(600),
                bid_size: 80,
                ask_size: 120,
                last: Some(550),
                volume: 3000,
                open_interest: 8000,
                iv: Some(0.27),
                delta: Some(-0.5),
            },
        }],
        timestamp_ms: 1704067200000,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"underlying\":\"AAPL\""));
    assert!(json.contains("\"expiration\":\"20240315\""));
}

// ============================================================================
// ExecutionsQuery Tests
// ============================================================================

#[test]
fn test_executions_query_default() {
    let query = ExecutionsQuery::default();

    assert!(query.from.is_none());
    assert!(query.to.is_none());
    assert!(query.underlying.is_none());
    assert!(query.symbol.is_none());
    assert!(query.side.is_none());
    assert_eq!(query.offset, 0);
}

// ============================================================================
// OrderListQuery Tests
// ============================================================================

#[test]
fn test_order_list_query_default() {
    let query = OrderListQuery::default();

    assert!(query.symbol.is_none());
    assert!(query.side.is_none());
    assert!(query.status.is_none());
    assert_eq!(query.offset, 0);
}

// ============================================================================
// CancelAllQuery Tests
// ============================================================================

#[test]
fn test_cancel_all_query_default() {
    let query = CancelAllQuery::default();

    assert!(query.symbol.is_none());
    assert!(query.side.is_none());
}

// ============================================================================
// PositionQuery Tests
// ============================================================================

#[test]
fn test_position_query_default() {
    let query = PositionQuery::default();

    assert!(query.underlying.is_none());
}

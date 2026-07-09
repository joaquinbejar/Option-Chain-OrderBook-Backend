//! HTTP client for the orderbook API.

use crate::error::Error;
use crate::types::*;
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use std::time::Duration;

#[cfg(test)]
mod tests;

/// Percent-encodes a single dynamic path segment so a reserved character in a
/// caller-supplied value (symbol, expiration, order id, …) cannot break out of
/// its path segment or inject `/`, `?`, or `#` delimiters into the request URL.
///
/// Uses `reqwest::Url` (the re-exported `url` crate) path-segment encoding, so it
/// needs no extra dependency. Every step is fallible-checked and never panics; on
/// the structurally-impossible failure of the constant base URL it falls back to
/// the raw segment.
fn encode_segment(segment: &str) -> String {
    match reqwest::Url::parse("http://segment.invalid/") {
        Ok(mut url) => {
            match url.path_segments_mut() {
                // `PathSegmentsMut::push` percent-encodes using the PATH segment
                // set; `pop_if_empty` removes the placeholder empty segment first.
                Ok(mut path) => {
                    path.pop_if_empty().push(segment);
                }
                Err(()) => return segment.to_string(),
            }
            url.path().trim_start_matches('/').to_string()
        }
        Err(_) => segment.to_string(),
    }
}

/// Client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the API (e.g., "http://localhost:8080").
    pub base_url: String,
    /// Request timeout.
    pub timeout: Duration,
    /// Optional JWT bearer token. When set, it is sent as
    /// `Authorization: Bearer <token>` on REST requests and appended as
    /// `?token=<token>` on the WebSocket URL.
    pub token: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout: Duration::from_secs(30),
            token: None,
        }
    }
}

/// HTTP client for the Option Chain OrderBook API.
#[derive(Debug, Clone)]
pub struct OrderbookClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl OrderbookClient {
    /// Creates a new client with the given configuration.
    ///
    /// When `config.token` is set, every REST request carries an
    /// `Authorization: Bearer` header.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be built (including an invalid
    /// bearer token that cannot be encoded as a header value).
    pub fn new(config: ClientConfig) -> Result<Self, Error> {
        let mut builder = Client::builder().timeout(config.timeout);
        if let Some(token) = &config.token {
            let mut headers = HeaderMap::new();
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| Error::InvalidRequest("invalid bearer token".to_string()))?;
            headers.insert(AUTHORIZATION, value);
            builder = builder.default_headers(headers);
        }
        let client = builder.build()?;

        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            token: config.token,
        })
    }

    /// Creates a new client with default configuration.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be built.
    pub fn with_base_url(base_url: &str) -> Result<Self, Error> {
        Self::new(ClientConfig {
            base_url: base_url.to_string(),
            ..Default::default()
        })
    }

    /// Creates a new client for `base_url` carrying the given JWT bearer token.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be built.
    pub fn with_token(base_url: &str, token: &str) -> Result<Self, Error> {
        Self::new(ClientConfig {
            base_url: base_url.to_string(),
            token: Some(token.to_string()),
            ..Default::default()
        })
    }

    // ========================================================================
    // Health & Stats
    // ========================================================================

    /// Performs a health check.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn health_check(&self) -> Result<HealthResponse, Error> {
        let url = format!("{}/health", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets global statistics.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_global_stats(&self) -> Result<GlobalStatsResponse, Error> {
        let url = format!("{}/api/v1/stats", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Underlyings
    // ========================================================================

    /// Lists all underlyings.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_underlyings(&self) -> Result<UnderlyingsListResponse, Error> {
        let url = format!("{}/api/v1/underlyings", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Creates a new underlying.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn create_underlying(&self, symbol: &str) -> Result<UnderlyingSummary, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}",
            self.base_url,
            encode_segment(symbol)
        );
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets an underlying by symbol.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_underlying(&self, symbol: &str) -> Result<UnderlyingSummary, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}",
            self.base_url,
            encode_segment(symbol)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Deletes an underlying.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn delete_underlying(&self, symbol: &str) -> Result<DeleteUnderlyingResponse, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}",
            self.base_url,
            encode_segment(symbol)
        );
        let resp = self.client.delete(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Expirations
    // ========================================================================

    /// Lists expirations for an underlying.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_expirations(
        &self,
        underlying: &str,
    ) -> Result<ExpirationsListResponse, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations",
            self.base_url,
            encode_segment(underlying)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Creates a new expiration.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn create_expiration(
        &self,
        underlying: &str,
        expiration: &str,
    ) -> Result<ExpirationSummary, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}",
            self.base_url,
            encode_segment(underlying),
            encode_segment(expiration)
        );
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets an expiration.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_expiration(
        &self,
        underlying: &str,
        expiration: &str,
    ) -> Result<ExpirationSummary, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}",
            self.base_url,
            encode_segment(underlying),
            encode_segment(expiration)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Strikes
    // ========================================================================

    /// Lists strikes for an expiration.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_strikes(
        &self,
        underlying: &str,
        expiration: &str,
    ) -> Result<StrikesListResponse, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes",
            self.base_url,
            encode_segment(underlying),
            encode_segment(expiration)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Creates a new strike.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn create_strike(
        &self,
        underlying: &str,
        expiration: &str,
        strike: u64,
    ) -> Result<StrikeSummary, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}",
            self.base_url,
            encode_segment(underlying),
            encode_segment(expiration),
            strike
        );
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets a strike.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_strike(
        &self,
        underlying: &str,
        expiration: &str,
        strike: u64,
    ) -> Result<StrikeSummary, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}",
            self.base_url,
            encode_segment(underlying),
            encode_segment(expiration),
            strike
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Options
    // ========================================================================

    /// Gets an option order book.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_option_book(
        &self,
        path: &OptionPath,
    ) -> Result<OrderBookSnapshotResponse, Error> {
        let url = self.option_base(path);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets an option quote.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_option_quote(&self, path: &OptionPath) -> Result<QuoteResponse, Error> {
        let url = format!("{}/quote", self.option_base(path));
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Orders
    // ========================================================================

    /// Adds a limit order.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn add_order(
        &self,
        path: &OptionPath,
        request: &AddOrderRequest,
    ) -> Result<AddOrderResponse, Error> {
        let url = format!("{}/orders", self.option_base(path));
        let resp = self.client.post(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Submits a market order.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn submit_market_order(
        &self,
        path: &OptionPath,
        request: &MarketOrderRequest,
    ) -> Result<MarketOrderResponse, Error> {
        let url = format!("{}/orders/market", self.option_base(path));
        let resp = self.client.post(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Cancels an order.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn cancel_order(
        &self,
        path: &OptionPath,
        order_id: &str,
    ) -> Result<CancelOrderResponse, Error> {
        let url = format!(
            "{}/orders/{}",
            self.option_base(path),
            encode_segment(order_id)
        );
        let resp = self.client.delete(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets an enriched order book snapshot with configurable depth.
    ///
    /// # Arguments
    /// * `path` - Option path (underlying, expiration, strike, style)
    /// * `depth` - Depth parameter: "top" (default), "10", "20", or "full"
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_option_snapshot(
        &self,
        path: &OptionPath,
        depth: Option<&str>,
    ) -> Result<EnrichedSnapshotResponse, Error> {
        let mut url = format!("{}/snapshot", self.option_base(path));
        if let Some(d) = depth {
            url.push_str(&format!("?depth={}", d));
        }
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Controls
    // ========================================================================

    /// Gets current system control status.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_controls(&self) -> Result<SystemControlResponse, Error> {
        let url = format!("{}/api/v1/controls", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Activates the kill switch (disables all quoting).
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn kill_switch(&self) -> Result<KillSwitchResponse, Error> {
        let url = format!("{}/api/v1/controls/kill-switch", self.base_url);
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Enables quoting (deactivates kill switch).
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn enable_quoting(&self) -> Result<KillSwitchResponse, Error> {
        let url = format!("{}/api/v1/controls/enable", self.base_url);
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Updates global parameters.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn update_parameters(
        &self,
        request: &UpdateParametersRequest,
    ) -> Result<UpdateParametersResponse, Error> {
        let url = format!("{}/api/v1/controls/parameters", self.base_url);
        let resp = self.client.post(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Lists all instruments with their status.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_instruments(&self) -> Result<InstrumentsListResponse, Error> {
        let url = format!("{}/api/v1/controls/instruments", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Toggles quoting for a specific instrument.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn toggle_instrument(&self, symbol: &str) -> Result<InstrumentToggleResponse, Error> {
        let url = format!(
            "{}/api/v1/controls/instrument/{}/toggle",
            self.base_url,
            encode_segment(symbol)
        );
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Prices
    // ========================================================================

    /// Inserts a new underlying price.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn insert_price(
        &self,
        request: &InsertPriceRequest,
    ) -> Result<InsertPriceResponse, Error> {
        let url = format!("{}/api/v1/prices", self.base_url);
        let resp = self.client.post(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Gets the latest price for a symbol.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_latest_price(&self, symbol: &str) -> Result<LatestPriceResponse, Error> {
        let url = format!("{}/api/v1/prices/{}", self.base_url, encode_segment(symbol));
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets prices for all symbols.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_all_prices(&self) -> Result<Vec<LatestPriceResponse>, Error> {
        let url = format!("{}/api/v1/prices", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Authentication
    // ========================================================================

    /// Issues a signed JWT via `POST /api/v1/auth/token`.
    ///
    /// Requires the operator bootstrap secret in `request.secret`. This call does
    /// not require an existing token.
    ///
    /// # Errors
    /// Returns error if the request fails or the server rejects the secret.
    pub async fn issue_token(&self, request: &TokenRequest) -> Result<TokenResponse, Error> {
        let url = format!("{}/api/v1/auth/token", self.base_url);
        let resp = self.client.post(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Executions
    // ========================================================================

    /// Lists executions with optional filters.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_executions(
        &self,
        query: Option<&ExecutionsQuery>,
    ) -> Result<ExecutionsListResponse, Error> {
        let mut url = format!("{}/api/v1/executions", self.base_url);
        if let Some(q) = query {
            let params = serde_urlencoded::to_string(q).unwrap_or_default();
            if !params.is_empty() {
                url.push_str(&format!("?{}", params));
            }
        }
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets a specific execution by ID.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_execution(&self, execution_id: &str) -> Result<ExecutionInfo, Error> {
        let url = format!(
            "{}/api/v1/executions/{}",
            self.base_url,
            encode_segment(execution_id)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Positions
    // ========================================================================

    /// Lists positions with optional filters.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_positions(
        &self,
        query: Option<&PositionQuery>,
    ) -> Result<PositionsListResponse, Error> {
        let mut url = format!("{}/api/v1/positions", self.base_url);
        if let Some(q) = query {
            let params = serde_urlencoded::to_string(q).unwrap_or_default();
            if !params.is_empty() {
                url.push_str(&format!("?{}", params));
            }
        }
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets a specific position by symbol.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_position(&self, symbol: &str) -> Result<PositionResponse, Error> {
        let url = format!(
            "{}/api/v1/positions/{}",
            self.base_url,
            encode_segment(symbol)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Orderbook Snapshots (Persistence)
    // ========================================================================

    /// Creates a snapshot of all orderbooks.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn create_snapshot(&self) -> Result<CreateSnapshotResponse, Error> {
        let url = format!("{}/api/v1/admin/snapshot", self.base_url);
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Lists all snapshots.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_snapshots(&self) -> Result<SnapshotsListResponse, Error> {
        let url = format!("{}/api/v1/admin/snapshots", self.base_url);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets a specific snapshot by ID.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Vec<OrderbookSnapshotInfo>, Error> {
        let url = format!(
            "{}/api/v1/admin/snapshots/{}",
            self.base_url,
            encode_segment(snapshot_id)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Restores orderbooks from a snapshot.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn restore_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<RestoreSnapshotResponse, Error> {
        let url = format!(
            "{}/api/v1/admin/snapshots/{}/restore",
            self.base_url,
            encode_segment(snapshot_id)
        );
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Orders (Extended)
    // ========================================================================

    /// Lists orders with optional filters.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn list_orders(
        &self,
        query: Option<&OrderListQuery>,
    ) -> Result<OrderListResponse, Error> {
        let mut url = format!("{}/api/v1/orders", self.base_url);
        if let Some(q) = query {
            let params = serde_urlencoded::to_string(q).unwrap_or_default();
            if !params.is_empty() {
                url.push_str(&format!("?{}", params));
            }
        }
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets order status by ID.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_order_status(&self, order_id: &str) -> Result<OrderStatusResponse, Error> {
        let url = format!(
            "{}/api/v1/orders/{}",
            self.base_url,
            encode_segment(order_id)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Modifies an existing order.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn modify_order(
        &self,
        path: &OptionPath,
        order_id: &str,
        request: &ModifyOrderRequest,
    ) -> Result<ModifyOrderResponse, Error> {
        let url = format!(
            "{}/orders/{}",
            self.option_base(path),
            encode_segment(order_id)
        );
        let resp = self.client.patch(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Submits multiple orders in bulk.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn bulk_submit_orders(
        &self,
        request: &BulkOrderRequest,
    ) -> Result<BulkOrderResponse, Error> {
        let url = format!("{}/api/v1/orders/bulk", self.base_url);
        let resp = self.client.post(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Cancels multiple orders in bulk.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn bulk_cancel_orders(
        &self,
        request: &BulkCancelRequest,
    ) -> Result<BulkCancelResponse, Error> {
        let url = format!("{}/api/v1/orders/bulk", self.base_url);
        let resp = self.client.delete(&url).json(request).send().await?;
        self.handle_response(resp).await
    }

    /// Cancels all orders with optional filters.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn cancel_all_orders(
        &self,
        query: Option<&CancelAllQuery>,
    ) -> Result<CancelAllResponse, Error> {
        let mut url = format!("{}/api/v1/orders/cancel-all", self.base_url);
        if let Some(q) = query {
            let params = serde_urlencoded::to_string(q).unwrap_or_default();
            if !params.is_empty() {
                url.push_str(&format!("?{}", params));
            }
        }
        let resp = self.client.delete(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Greeks
    // ========================================================================

    /// Gets option greeks.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_option_greeks(&self, path: &OptionPath) -> Result<GreeksResponse, Error> {
        let url = format!("{}/greeks", self.option_base(path));
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Last Trade
    // ========================================================================

    /// Gets the last trade for an option.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_last_trade(&self, path: &OptionPath) -> Result<LastTradeResponse, Error> {
        let url = format!("{}/last-trade", self.option_base(path));
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // OHLC
    // ========================================================================

    /// Gets OHLC candlestick data for an option.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_ohlc(
        &self,
        path: &OptionPath,
        query: Option<&OhlcQuery>,
    ) -> Result<OhlcResponse, Error> {
        let mut url = format!("{}/ohlc", self.option_base(path));
        if let Some(q) = query {
            let params = serde_urlencoded::to_string(q).unwrap_or_default();
            if !params.is_empty() {
                url.push_str(&format!("?{}", params));
            }
        }
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Orderbook Metrics
    // ========================================================================

    /// Gets orderbook metrics for an option.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_orderbook_metrics(
        &self,
        path: &OptionPath,
    ) -> Result<OrderbookMetricsResponse, Error> {
        let url = format!("{}/metrics", self.option_base(path));
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Volatility Surface
    // ========================================================================

    /// Gets the volatility surface for an underlying.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_volatility_surface(
        &self,
        underlying: &str,
    ) -> Result<VolatilitySurfaceResponse, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/volatility-surface",
            self.base_url,
            encode_segment(underlying)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // Option Chain
    // ========================================================================

    /// Gets the option chain for an expiration.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_option_chain(
        &self,
        underlying: &str,
        expiration: &str,
    ) -> Result<OptionChainResponse, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/chain",
            self.base_url,
            encode_segment(underlying),
            encode_segment(expiration)
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    // ========================================================================
    // WebSocket
    // ========================================================================

    /// Returns the WebSocket URL for this client.
    ///
    /// When the client carries a JWT, it is appended as `?token=<jwt>` so the
    /// browser-style upgrade authenticates.
    #[must_use]
    pub fn ws_url(&self) -> String {
        let ws_base = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        match &self.token {
            Some(token) => format!("{ws_base}/ws?token={token}"),
            None => format!("{ws_base}/ws"),
        }
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Builds the `.../options/{style}` URL prefix for an option, percent-encoding
    /// every dynamic segment so a reserved character cannot alter the path.
    fn option_base(&self, path: &OptionPath) -> String {
        format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}",
            self.base_url,
            encode_segment(&path.underlying),
            encode_segment(&path.expiration),
            path.strike,
            encode_segment(&path.style),
        )
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, Error> {
        let status = resp.status();

        if status.is_success() {
            Ok(resp.json().await?)
        } else if status.as_u16() == 404 {
            let text = resp.text().await.unwrap_or_default();
            Err(Error::NotFound(text))
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(Error::Api {
                status: status.as_u16(),
                message: text,
            })
        }
    }
}

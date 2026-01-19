//! HTTP client for the orderbook API.

use crate::error::Error;
use crate::types::*;
use reqwest::Client;
use std::time::Duration;

/// Client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the API (e.g., "http://localhost:8080").
    pub base_url: String,
    /// Request timeout.
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout: Duration::from_secs(30),
        }
    }
}

/// HTTP client for the Option Chain OrderBook API.
#[derive(Debug, Clone)]
pub struct OrderbookClient {
    client: Client,
    base_url: String,
}

impl OrderbookClient {
    /// Creates a new client with the given configuration.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be built.
    pub fn new(config: ClientConfig) -> Result<Self, Error> {
        let client = Client::builder().timeout(config.timeout).build()?;

        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
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
        let url = format!("{}/api/v1/underlyings/{}", self.base_url, symbol);
        let resp = self.client.post(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets an underlying by symbol.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_underlying(&self, symbol: &str) -> Result<UnderlyingSummary, Error> {
        let url = format!("{}/api/v1/underlyings/{}", self.base_url, symbol);
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Deletes an underlying.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn delete_underlying(&self, symbol: &str) -> Result<(), Error> {
        let url = format!("{}/api/v1/underlyings/{}", self.base_url, symbol);
        let resp = self.client.delete(&url).send().await?;
        self.handle_empty_response(resp).await
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
            self.base_url, underlying
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
            self.base_url, underlying, expiration
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
            self.base_url, underlying, expiration
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
            self.base_url, underlying, expiration
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
            self.base_url, underlying, expiration, strike
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
            self.base_url, underlying, expiration, strike
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
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}",
            self.base_url, path.underlying, path.expiration, path.strike, path.style
        );
        let resp = self.client.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// Gets an option quote.
    ///
    /// # Errors
    /// Returns error if the request fails.
    pub async fn get_option_quote(&self, path: &OptionPath) -> Result<QuoteResponse, Error> {
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}/quote",
            self.base_url, path.underlying, path.expiration, path.strike, path.style
        );
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
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}/orders",
            self.base_url, path.underlying, path.expiration, path.strike, path.style
        );
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
        let url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}/orders/market",
            self.base_url, path.underlying, path.expiration, path.strike, path.style
        );
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
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}/orders/{}",
            self.base_url, path.underlying, path.expiration, path.strike, path.style, order_id
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
        let mut url = format!(
            "{}/api/v1/underlyings/{}/expirations/{}/strikes/{}/options/{}/snapshot",
            self.base_url, path.underlying, path.expiration, path.strike, path.style
        );
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
            self.base_url, symbol
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
        let url = format!("{}/api/v1/prices/{}", self.base_url, symbol);
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
    // WebSocket
    // ========================================================================

    /// Returns the WebSocket URL for this client.
    #[must_use]
    pub fn ws_url(&self) -> String {
        let ws_base = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        format!("{}/ws", ws_base)
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

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

    async fn handle_empty_response(&self, resp: reqwest::Response) -> Result<(), Error> {
        let status = resp.status();

        if status.is_success() {
            Ok(())
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

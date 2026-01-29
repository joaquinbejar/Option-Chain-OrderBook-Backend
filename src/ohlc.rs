//! OHLC (Open, High, Low, Close) candlestick data aggregation.
//!
//! This module provides functionality to aggregate trades into OHLC bars
//! at configurable intervals for charting and technical analysis.

use crate::models::{OhlcBar, OhlcInterval};
use dashmap::DashMap;
use std::collections::BTreeMap;

/// Key for storing OHLC bars: (symbol, interval).
type BarKey = (String, OhlcInterval);

/// OHLC aggregator that collects trades and produces candlestick bars.
///
/// Bars are stored in memory, keyed by symbol and interval.
/// Each symbol/interval combination has a sorted map of bars by timestamp.
#[derive(Debug, Default)]
pub struct OhlcAggregator {
    /// Storage for OHLC bars: (symbol, interval) -> (timestamp -> bar).
    bars: DashMap<BarKey, BTreeMap<u64, OhlcBar>>,
}

impl OhlcAggregator {
    /// Creates a new OHLC aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bars: DashMap::new(),
        }
    }

    /// Records a trade and updates the appropriate OHLC bars.
    ///
    /// This updates bars for all intervals (1m, 5m, 15m, 1h, 4h, 1d).
    ///
    /// # Arguments
    ///
    /// * `symbol` - The symbol identifier (e.g., "AAPL_20251231_150_C")
    /// * `timestamp_ms` - Trade timestamp in milliseconds since epoch
    /// * `price` - Trade price in smallest units
    /// * `quantity` - Trade quantity
    pub fn record_trade(&self, symbol: &str, timestamp_ms: u64, price: u128, quantity: u64) {
        let timestamp_secs = timestamp_ms / 1000;

        // Update bars for all intervals
        for interval in [
            OhlcInterval::OneMinute,
            OhlcInterval::FiveMinutes,
            OhlcInterval::FifteenMinutes,
            OhlcInterval::OneHour,
            OhlcInterval::FourHours,
            OhlcInterval::OneDay,
        ] {
            self.update_bar(symbol, interval, timestamp_secs, price, quantity);
        }
    }

    /// Updates a single bar for a specific interval.
    fn update_bar(
        &self,
        symbol: &str,
        interval: OhlcInterval,
        timestamp_secs: u64,
        price: u128,
        quantity: u64,
    ) {
        let bar_timestamp = interval.floor_timestamp(timestamp_secs);
        let key = (symbol.to_string(), interval);

        self.bars
            .entry(key)
            .or_default()
            .entry(bar_timestamp)
            .and_modify(|bar| bar.update(price, quantity))
            .or_insert_with(|| OhlcBar::new(bar_timestamp, price, quantity));
    }

    /// Gets OHLC bars for a symbol and interval within a time range.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The symbol identifier
    /// * `interval` - The bar interval
    /// * `from` - Optional start timestamp in seconds (inclusive)
    /// * `to` - Optional end timestamp in seconds (inclusive)
    /// * `limit` - Maximum number of bars to return
    ///
    /// # Returns
    ///
    /// A vector of OHLC bars sorted by timestamp (oldest first).
    #[must_use]
    pub fn get_bars(
        &self,
        symbol: &str,
        interval: OhlcInterval,
        from: Option<u64>,
        to: Option<u64>,
        limit: usize,
    ) -> Vec<OhlcBar> {
        let key = (symbol.to_string(), interval);

        let Some(bars_map) = self.bars.get(&key) else {
            return Vec::new();
        };

        let from_ts = from.unwrap_or(0);
        let to_ts = to.unwrap_or(u64::MAX);

        bars_map
            .range(from_ts..=to_ts)
            .map(|(_, bar)| *bar)
            .take(limit)
            .collect()
    }

    /// Gets the most recent bar for a symbol and interval.
    #[must_use]
    pub fn get_latest_bar(&self, symbol: &str, interval: OhlcInterval) -> Option<OhlcBar> {
        let key = (symbol.to_string(), interval);
        self.bars.get(&key)?.iter().next_back().map(|(_, bar)| *bar)
    }

    /// Returns the number of bars stored for a symbol and interval.
    #[must_use]
    pub fn bar_count(&self, symbol: &str, interval: OhlcInterval) -> usize {
        let key = (symbol.to_string(), interval);
        self.bars.get(&key).map_or(0, |m| m.len())
    }

    /// Clears all bars for a symbol.
    pub fn clear_symbol(&self, symbol: &str) {
        for interval in [
            OhlcInterval::OneMinute,
            OhlcInterval::FiveMinutes,
            OhlcInterval::FifteenMinutes,
            OhlcInterval::OneHour,
            OhlcInterval::FourHours,
            OhlcInterval::OneDay,
        ] {
            let key = (symbol.to_string(), interval);
            self.bars.remove(&key);
        }
    }

    /// Clears all stored bars.
    pub fn clear_all(&self) {
        self.bars.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ohlc_aggregator_new() {
        let aggregator = OhlcAggregator::new();
        assert_eq!(aggregator.bar_count("TEST", OhlcInterval::OneMinute), 0);
    }

    #[test]
    fn test_record_single_trade() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let timestamp_ms = 1704067200000; // 2024-01-01 00:00:00 UTC
        let price = 500;
        let quantity = 100;

        aggregator.record_trade(symbol, timestamp_ms, price, quantity);

        // Check 1m bar
        let bars = aggregator.get_bars(symbol, OhlcInterval::OneMinute, None, None, 100);
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].open, 500);
        assert_eq!(bars[0].high, 500);
        assert_eq!(bars[0].low, 500);
        assert_eq!(bars[0].close, 500);
        assert_eq!(bars[0].volume, 100);
        assert_eq!(bars[0].trade_count, 1);
    }

    #[test]
    fn test_record_multiple_trades_same_bar() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts = 1704067200000; // 2024-01-01 00:00:00 UTC

        // Three trades within the same minute
        aggregator.record_trade(symbol, base_ts, 500, 100);
        aggregator.record_trade(symbol, base_ts + 10000, 520, 50); // +10 seconds
        aggregator.record_trade(symbol, base_ts + 30000, 490, 75); // +30 seconds

        let bars = aggregator.get_bars(symbol, OhlcInterval::OneMinute, None, None, 100);
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].open, 500);
        assert_eq!(bars[0].high, 520);
        assert_eq!(bars[0].low, 490);
        assert_eq!(bars[0].close, 490);
        assert_eq!(bars[0].volume, 225); // 100 + 50 + 75
        assert_eq!(bars[0].trade_count, 3);
    }

    #[test]
    fn test_record_trades_different_bars() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts = 1704067200000; // 2024-01-01 00:00:00 UTC

        // Trades in different minutes
        aggregator.record_trade(symbol, base_ts, 500, 100);
        aggregator.record_trade(symbol, base_ts + 60000, 510, 50); // +1 minute
        aggregator.record_trade(symbol, base_ts + 120000, 520, 75); // +2 minutes

        let bars = aggregator.get_bars(symbol, OhlcInterval::OneMinute, None, None, 100);
        assert_eq!(bars.len(), 3);

        // Bars should be sorted by timestamp
        assert!(bars[0].timestamp < bars[1].timestamp);
        assert!(bars[1].timestamp < bars[2].timestamp);
    }

    #[test]
    fn test_get_bars_with_time_range() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts = 1704067200000; // 2024-01-01 00:00:00 UTC

        // Create 5 bars
        for i in 0..5 {
            aggregator.record_trade(symbol, base_ts + i * 60000, 500 + i as u128, 100);
        }

        // Get bars from minute 1 to minute 3
        let from_ts = 1704067260; // +1 minute in seconds
        let to_ts = 1704067380; // +3 minutes in seconds
        let bars = aggregator.get_bars(
            symbol,
            OhlcInterval::OneMinute,
            Some(from_ts),
            Some(to_ts),
            100,
        );

        assert_eq!(bars.len(), 3);
    }

    #[test]
    fn test_get_bars_with_limit() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts = 1704067200000;

        // Create 10 bars
        for i in 0..10 {
            aggregator.record_trade(symbol, base_ts + i * 60000, 500, 100);
        }

        // Limit to 5 bars
        let bars = aggregator.get_bars(symbol, OhlcInterval::OneMinute, None, None, 5);
        assert_eq!(bars.len(), 5);
    }

    #[test]
    fn test_get_latest_bar() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts = 1704067200000;

        aggregator.record_trade(symbol, base_ts, 500, 100);
        aggregator.record_trade(symbol, base_ts + 60000, 510, 50);
        aggregator.record_trade(symbol, base_ts + 120000, 520, 75);

        let latest = aggregator.get_latest_bar(symbol, OhlcInterval::OneMinute);
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().close, 520);
    }

    #[test]
    fn test_get_latest_bar_empty() {
        let aggregator = OhlcAggregator::new();
        let latest = aggregator.get_latest_bar("NONEXISTENT", OhlcInterval::OneMinute);
        assert!(latest.is_none());
    }

    #[test]
    fn test_multiple_intervals() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let timestamp_ms = 1704067200000;

        aggregator.record_trade(symbol, timestamp_ms, 500, 100);

        // All intervals should have a bar
        assert_eq!(aggregator.bar_count(symbol, OhlcInterval::OneMinute), 1);
        assert_eq!(aggregator.bar_count(symbol, OhlcInterval::FiveMinutes), 1);
        assert_eq!(
            aggregator.bar_count(symbol, OhlcInterval::FifteenMinutes),
            1
        );
        assert_eq!(aggregator.bar_count(symbol, OhlcInterval::OneHour), 1);
        assert_eq!(aggregator.bar_count(symbol, OhlcInterval::FourHours), 1);
        assert_eq!(aggregator.bar_count(symbol, OhlcInterval::OneDay), 1);
    }

    #[test]
    fn test_clear_symbol() {
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";

        aggregator.record_trade(symbol, 1704067200000, 500, 100);
        assert!(aggregator.bar_count(symbol, OhlcInterval::OneMinute) > 0);

        aggregator.clear_symbol(symbol);
        assert_eq!(aggregator.bar_count(symbol, OhlcInterval::OneMinute), 0);
    }

    #[test]
    fn test_clear_all() {
        let aggregator = OhlcAggregator::new();

        aggregator.record_trade("SYM1", 1704067200000, 500, 100);
        aggregator.record_trade("SYM2", 1704067200000, 600, 200);

        aggregator.clear_all();

        assert_eq!(aggregator.bar_count("SYM1", OhlcInterval::OneMinute), 0);
        assert_eq!(aggregator.bar_count("SYM2", OhlcInterval::OneMinute), 0);
    }

    #[test]
    fn test_interval_floor_timestamp() {
        // 1m interval
        assert_eq!(
            OhlcInterval::OneMinute.floor_timestamp(1704067265),
            1704067260
        );

        // 5m interval
        assert_eq!(
            OhlcInterval::FiveMinutes.floor_timestamp(1704067265),
            1704067200
        );

        // 1h interval
        assert_eq!(
            OhlcInterval::OneHour.floor_timestamp(1704069000),
            1704067200
        );

        // 1d interval
        assert_eq!(OhlcInterval::OneDay.floor_timestamp(1704100000), 1704067200);
    }
}

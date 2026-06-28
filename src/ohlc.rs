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
    /// # Limit semantics
    ///
    /// When more than `limit` bars match the `[from, to]` range:
    /// - **Open-ended** (`to` is `None`): the caller wants "the latest N", so the
    ///   NEWEST `limit` bars are returned (issue #73). Without this a charting
    ///   client polling recent data would receive the OLDEST `limit` bars once the
    ///   history grew past `limit` — backwards from candlestick semantics.
    /// - **Explicit `to`**: the result is anchored at `from` (the window start),
    ///   so the OLDEST `limit` bars within `[from, to]` are returned (unchanged
    ///   behavior).
    ///
    /// # Returns
    ///
    /// A vector of OHLC bars sorted by timestamp (oldest first) in both cases.
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
        let range = bars_map.range(from_ts..=to_ts);

        if to.is_none() {
            // Open-ended: take the newest `limit` bars from the end of the range,
            // then restore ascending (oldest-first) order for the returned Vec.
            let mut bars: Vec<OhlcBar> = range.rev().take(limit).map(|(_, bar)| *bar).collect();
            bars.reverse();
            bars
        } else {
            // Explicit end: the window ends at `to`; keep the oldest `limit` bars.
            range.take(limit).map(|(_, bar)| *bar).collect()
        }
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
    fn test_get_bars_open_ended_returns_newest_n() {
        // Issue #73: with a limit and no explicit `to`, return the NEWEST `limit`
        // bars (still ascending) — NOT the oldest. Fails under the old
        // oldest-first `.take(limit)`.
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts_ms: u64 = 1704067200000; // minute boundary
        let total: u64 = 600;
        let limit: usize = 500;

        for i in 0..total {
            aggregator.record_trade(symbol, base_ts_ms + i * 60_000, 500 + i as u128, 100);
        }

        let bars = aggregator.get_bars(symbol, OhlcInterval::OneMinute, None, None, limit);
        assert_eq!(bars.len(), limit, "returns exactly `limit` bars");

        let base_secs = base_ts_ms / 1000;
        // The newest 500 of 600 bars are indices 100..=599.
        let expected_first = base_secs + 100 * 60;
        let expected_last = base_secs + (total - 1) * 60;
        assert_eq!(
            bars[0].timestamp, expected_first,
            "first bar is the newest window start, not the oldest"
        );
        assert_eq!(
            bars[bars.len() - 1].timestamp,
            expected_last,
            "last bar is the most recent"
        );
        // Explicitly NOT the oldest bar.
        assert_ne!(
            bars[0].timestamp, base_secs,
            "must not return the oldest bars"
        );
        // Ordering preserved: strictly ascending by timestamp.
        assert!(
            bars.windows(2).all(|w| w[0].timestamp < w[1].timestamp),
            "ascending (oldest-first) order preserved"
        );
    }

    #[test]
    fn test_get_bars_with_explicit_to_returns_oldest_limit_within_window() {
        // Issue #73: when `to` is given the window is anchored at `to`, so the
        // OLDEST `limit` within [from, to] are returned (unchanged semantics).
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts_ms: u64 = 1704067200000;

        for i in 0..10u64 {
            aggregator.record_trade(symbol, base_ts_ms + i * 60_000, 500, 100);
        }

        let base_secs = base_ts_ms / 1000;
        // Window [bar2 .. bar7] inclusive = 6 bars; limit 3 -> oldest 3 of window.
        let from_ts = base_secs + 2 * 60;
        let to_ts = base_secs + 7 * 60;
        let bars = aggregator.get_bars(
            symbol,
            OhlcInterval::OneMinute,
            Some(from_ts),
            Some(to_ts),
            3,
        );

        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0].timestamp, from_ts, "window starts at `from`");
        assert_eq!(
            bars[2].timestamp,
            base_secs + 4 * 60,
            "oldest 3 bars within the explicit window"
        );
        assert!(bars.windows(2).all(|w| w[0].timestamp < w[1].timestamp));
    }

    #[test]
    fn test_get_bars_limit_exceeds_available_returns_all_ascending() {
        // Issue #73: a limit larger than the number of bars returns them all,
        // ascending.
        let aggregator = OhlcAggregator::new();
        let symbol = "AAPL_20251231_150_C";
        let base_ts_ms: u64 = 1704067200000;

        for i in 0..5u64 {
            aggregator.record_trade(symbol, base_ts_ms + i * 60_000, 500, 100);
        }

        let base_secs = base_ts_ms / 1000;
        let bars = aggregator.get_bars(symbol, OhlcInterval::OneMinute, None, None, 500);

        assert_eq!(
            bars.len(),
            5,
            "returns all available when limit exceeds count"
        );
        assert_eq!(bars[0].timestamp, base_secs, "oldest first");
        assert_eq!(bars[4].timestamp, base_secs + 4 * 60, "newest last");
        assert!(bars.windows(2).all(|w| w[0].timestamp < w[1].timestamp));
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

    #[test]
    fn test_ohlc_bar_update_saturates_on_overflow() {
        // Issue #61: bars are aggregated from already-executed trades, so an
        // overflow of the running volume / trade_count must saturate (no panic,
        // no wrap) rather than be rejected.
        let mut bar = OhlcBar::new(0, 100, u64::MAX);
        bar.update(120, 10);

        assert_eq!(bar.volume, u64::MAX, "volume saturates at u64::MAX");
        assert_eq!(bar.trade_count, 2, "trade_count still advances");
        assert_eq!(bar.close, 120);
        assert_eq!(bar.high, 120);

        // trade_count saturation: drive it to u64::MAX, then one more trade.
        bar.trade_count = u64::MAX;
        bar.update(130, 0);
        assert_eq!(
            bar.trade_count,
            u64::MAX,
            "trade_count saturates at u64::MAX"
        );
    }
}

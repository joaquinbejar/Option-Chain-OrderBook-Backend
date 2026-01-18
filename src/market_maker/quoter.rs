//! Quote generation for market making.

use crate::market_maker::OptionPricer;
use optionstratlib::{ExpirationDate, OptionStyle};

/// Quote parameters for a single option.
#[derive(Debug, Clone)]
pub struct QuoteParams {
    /// Bid price in cents.
    pub bid_price: u64,
    /// Ask price in cents.
    pub ask_price: u64,
    /// Bid size.
    pub bid_size: u64,
    /// Ask size.
    pub ask_size: u64,
}

/// Input parameters for quote generation.
#[derive(Debug, Clone)]
pub struct QuoteInput<'a> {
    /// Current underlying price in cents.
    pub spot_cents: u64,
    /// Strike price in cents.
    pub strike_cents: u64,
    /// Time to expiration.
    pub expiration: &'a ExpirationDate,
    /// Call or Put.
    pub style: OptionStyle,
    /// Multiplier for the spread (1.0 = normal).
    pub spread_multiplier: f64,
    /// Scalar for quote size (0.0 to 1.0).
    pub size_scalar: f64,
    /// Skew factor (-1.0 to 1.0, positive = bullish).
    pub directional_skew: f64,
    /// Optional implied volatility.
    pub iv: Option<f64>,
}

/// Quoter generates bid/ask quotes for options.
pub struct Quoter {
    pricer: OptionPricer,
    /// Base spread in basis points (e.g., 50 = 0.5%).
    base_spread_bps: u64,
    /// Base quote size.
    base_size: u64,
}

impl Quoter {
    /// Creates a new quoter.
    ///
    /// # Arguments
    /// * `pricer` - Option pricer for theoretical values
    /// * `base_spread_bps` - Base spread in basis points
    /// * `base_size` - Base quote size
    #[must_use]
    pub fn new(pricer: OptionPricer, base_spread_bps: u64, base_size: u64) -> Self {
        Self {
            pricer,
            base_spread_bps,
            base_size,
        }
    }

    /// Generates a two-sided quote for an option.
    ///
    /// # Arguments
    /// * `input` - Quote input parameters
    ///
    /// # Returns
    /// Quote parameters with bid/ask prices and sizes.
    #[must_use]
    pub fn generate_quote(&self, input: &QuoteInput<'_>) -> QuoteParams {
        let spot = input.spot_cents as f64 / 100.0;
        let strike = input.strike_cents as f64 / 100.0;

        // Calculate theoretical value
        let theo =
            self.pricer
                .theoretical_value(spot, strike, input.expiration, input.style, input.iv);
        let theo_cents = (theo * 100.0).round() as u64;

        // Calculate spread based on theo value and base spread
        let half_spread_bps = (self.base_spread_bps as f64 * input.spread_multiplier / 2.0) as u64;
        let half_spread_cents =
            ((theo_cents as f64 * half_spread_bps as f64) / 10000.0).max(1.0) as u64;

        // Apply directional skew
        // Positive skew = bullish = tighter bid, wider ask for calls
        // Negative skew = bearish = wider bid, tighter ask for calls
        let skew_adjustment = (half_spread_cents as f64 * input.directional_skew * 0.5) as i64;

        let (bid_adjustment, ask_adjustment) = match input.style {
            OptionStyle::Call => (-skew_adjustment, skew_adjustment),
            OptionStyle::Put => (skew_adjustment, -skew_adjustment),
        };

        let bid_price =
            (theo_cents as i64 - half_spread_cents as i64 + bid_adjustment).max(1) as u64;
        let ask_price = (theo_cents as i64 + half_spread_cents as i64 + ask_adjustment)
            .max(bid_price as i64 + 1) as u64;

        // Calculate sizes with scalar
        let base_size = (self.base_size as f64 * input.size_scalar).max(1.0) as u64;

        // Adjust sizes based on skew (reduce size on the side we're less willing to trade)
        let skew_size_factor = 1.0 - input.directional_skew.abs() * 0.3;
        let (bid_size, ask_size) = if input.directional_skew > 0.0 {
            // Bullish: more willing to buy, less to sell
            (base_size, (base_size as f64 * skew_size_factor) as u64)
        } else if input.directional_skew < 0.0 {
            // Bearish: less willing to buy, more to sell
            ((base_size as f64 * skew_size_factor) as u64, base_size)
        } else {
            (base_size, base_size)
        };

        QuoteParams {
            bid_price,
            ask_price,
            bid_size: bid_size.max(1),
            ask_size: ask_size.max(1),
        }
    }

    /// Calculates the edge for a fill.
    ///
    /// # Arguments
    /// * `fill_price_cents` - Price at which the order was filled
    /// * `theo_cents` - Theoretical value at fill time
    /// * `side` - Buy or Sell
    ///
    /// # Returns
    /// Edge in cents (positive = favorable, negative = adverse).
    #[must_use]
    pub fn calculate_edge(fill_price_cents: u64, theo_cents: u64, is_buy: bool) -> i64 {
        if is_buy {
            // Buying: edge = theo - fill_price (we want to buy below theo)
            theo_cents as i64 - fill_price_cents as i64
        } else {
            // Selling: edge = fill_price - theo (we want to sell above theo)
            fill_price_cents as i64 - theo_cents as i64
        }
    }
}

impl Default for Quoter {
    fn default() -> Self {
        Self::new(OptionPricer::default(), 100, 10) // 1% spread, size 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optionstratlib::prelude::Positive;

    #[test]
    fn test_generate_quote() {
        let quoter = Quoter::default();
        let exp = ExpirationDate::Days(Positive::THIRTY);

        let input = QuoteInput {
            spot_cents: 10000,   // $100 spot
            strike_cents: 10000, // $100 strike
            expiration: &exp,
            style: OptionStyle::Call,
            spread_multiplier: 1.0,
            size_scalar: 1.0,
            directional_skew: 0.0,
            iv: Some(0.20),
        };

        let quote = quoter.generate_quote(&input);

        assert!(quote.bid_price < quote.ask_price);
        assert!(quote.bid_size > 0);
        assert!(quote.ask_size > 0);
    }

    #[test]
    fn test_bullish_skew() {
        let quoter = Quoter::default();
        let exp = ExpirationDate::Days(Positive::THIRTY);

        let neutral_input = QuoteInput {
            spot_cents: 10000,
            strike_cents: 10000,
            expiration: &exp,
            style: OptionStyle::Call,
            spread_multiplier: 1.0,
            size_scalar: 1.0,
            directional_skew: 0.0,
            iv: Some(0.20),
        };

        let bullish_input = QuoteInput {
            directional_skew: 0.5,
            ..neutral_input.clone()
        };

        let neutral = quoter.generate_quote(&neutral_input);
        let bullish = quoter.generate_quote(&bullish_input);

        // Bullish skew should have tighter bid (higher) for calls
        assert!(bullish.bid_price >= neutral.bid_price);
    }

    #[test]
    fn test_edge_calculation() {
        // Buying at 100 when theo is 105 = +5 edge
        assert_eq!(Quoter::calculate_edge(100, 105, true), 5);

        // Selling at 110 when theo is 105 = +5 edge
        assert_eq!(Quoter::calculate_edge(110, 105, false), 5);

        // Buying at 110 when theo is 105 = -5 edge (adverse)
        assert_eq!(Quoter::calculate_edge(110, 105, true), -5);
    }
}

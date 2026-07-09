//! Quote generation for market making.

use crate::market_maker::OptionPricer;
use optionstratlib::{ExpirationDate, OptionStyle};

/// Basis-points denominator: 1 basis point = 1/10_000, so a bps value is applied
/// to a price as `price * bps / BPS_DENOMINATOR`.
const BPS_DENOMINATOR: f64 = 10_000.0;

/// Fraction of the half-spread used as the maximum directional price skew.
///
/// The signed skew shift applied to both legs is `half_spread_cents *
/// directional_skew * SKEW_PRICE_WEIGHT`, so at full skew (`±1.0`) the parallel
/// shift is at most half the half-spread — it re-centers the quote without ever
/// crossing the theoretical value.
const SKEW_PRICE_WEIGHT: f64 = 0.5;

/// Weight controlling how much directional skew shrinks the size on the side the
/// maker is less willing to trade: that side's size is scaled by
/// `1 - directional_skew.abs() * SKEW_SIZE_WEIGHT`, i.e. down to 70% at full skew.
const SKEW_SIZE_WEIGHT: f64 = 0.3;

/// Default base spread in basis points (1%) for [`Quoter::default`].
const DEFAULT_BASE_SPREAD_BPS: u64 = 100;

/// Default base quote size for [`Quoter::default`].
const DEFAULT_BASE_SIZE: u64 = 10;

/// Quote parameters for a single option.
#[derive(Debug, Clone)]
pub struct QuoteParams {
    /// Bid price in cents.
    pub bid_price: u128,
    /// Ask price in cents.
    pub ask_price: u128,
    /// Bid size.
    pub bid_size: u64,
    /// Ask size.
    pub ask_size: u64,
    /// Theoretical value in cents the quote was built around. Stored with the
    /// resting orders so the captured edge can be computed when one fills
    /// (issue #69).
    pub theo_price: u64,
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
    /// `Some(QuoteParams)` with bid/ask prices (cents) and sizes, or `None` when
    /// the theoretical value is non-finite and no safe quote can be produced.
    ///
    /// # Boundary guard
    /// The Black-Scholes approximation works in `f64`. A degenerate input
    /// (zero/expired time-to-expiry, zero strike, or an extreme/non-finite vol)
    /// can make the theoretical value `NaN` or `±Inf`. Casting that straight to
    /// integer cents silently yields `0` (from `NaN`) or `u64::MAX` (from `+Inf`,
    /// which becomes `-1` as `i64`) and poisons every downstream bid/ask. To keep
    /// the `f64 → cents` boundary honest, a non-finite theoretical value (or a
    /// non-finite scaled-to-cents intermediate) returns `None` instead of a
    /// quote; the caller skips placing orders for that instrument. The value is
    /// never silently coerced to `0`.
    #[must_use]
    pub fn generate_quote(&self, input: &QuoteInput<'_>) -> Option<QuoteParams> {
        let spot = input.spot_cents as f64 / 100.0;
        let strike = input.strike_cents as f64 / 100.0;

        // Calculate theoretical value
        let theo =
            self.pricer
                .theoretical_value(spot, strike, input.expiration, input.style, input.iv);

        // Guard the f64 -> cents boundary: refuse to quote on a non-finite theo
        // (NaN / ±Inf) rather than casting it to a garbage cents value.
        if !theo.is_finite() {
            return None;
        }

        // Guard the scaled intermediate too: a finite-but-huge theo can overflow
        // to ±Inf when multiplied by 100, which must not slip into the cents cast.
        let theo_scaled = theo * 100.0;
        if !theo_scaled.is_finite() {
            return None;
        }
        let theo_cents = theo_scaled.round() as u64;

        // Calculate spread based on theo value and base spread
        let half_spread_bps = (self.base_spread_bps as f64 * input.spread_multiplier / 2.0) as u64;
        let half_spread_cents =
            ((theo_cents as f64 * half_spread_bps as f64) / BPS_DENOMINATOR).max(1.0) as u64;

        // Apply directional skew as a symmetric, same-signed PARALLEL shift of
        // both bid and ask (not a spread-widening). For a call, a positive
        // (bullish) skew raises both the bid and the ask by `skew_adjustment`:
        // relative to the theo midpoint the bid moves toward theo (tighter bid)
        // and the ask moves away from theo (wider ask); a negative (bearish)
        // skew lowers both by the same amount. A put's value moves opposite to
        // the underlying, so its sign is mirrored. Because the same adjustment
        // is applied to both legs, the quoted spread width is preserved.
        //
        // `skew_adjustment` is signed cents derived from a clamped skew in
        // [-1.0, 1.0]; its magnitude is at most `half_spread_cents * 0.5`. The
        // final prices are computed in `i64` and then floored with the existing
        // bid floor (`.max(1)`) and ask floor (`.max(bid + 1)`), so a negative
        // adjustment can never underflow the `u128` price.
        let skew_adjustment =
            (half_spread_cents as f64 * input.directional_skew * SKEW_PRICE_WEIGHT) as i64;

        let (bid_adjustment, ask_adjustment) = match input.style {
            OptionStyle::Call => (skew_adjustment, skew_adjustment),
            OptionStyle::Put => (-skew_adjustment, -skew_adjustment),
        };

        let bid_price =
            (theo_cents as i64 - half_spread_cents as i64 + bid_adjustment).max(1) as u128;
        let ask_price = (theo_cents as i64 + half_spread_cents as i64 + ask_adjustment)
            .max(bid_price as i64 + 1) as u128;

        // Calculate sizes with scalar
        let base_size = (self.base_size as f64 * input.size_scalar).max(1.0) as u64;

        // Adjust sizes based on skew (reduce size on the side we're less willing to trade)
        let skew_size_factor = 1.0 - input.directional_skew.abs() * SKEW_SIZE_WEIGHT;
        let (bid_size, ask_size) = if input.directional_skew > 0.0 {
            // Bullish: more willing to buy, less to sell
            (base_size, (base_size as f64 * skew_size_factor) as u64)
        } else if input.directional_skew < 0.0 {
            // Bearish: less willing to buy, more to sell
            ((base_size as f64 * skew_size_factor) as u64, base_size)
        } else {
            (base_size, base_size)
        };

        Some(QuoteParams {
            bid_price,
            ask_price,
            bid_size: bid_size.max(1),
            ask_size: ask_size.max(1),
            theo_price: theo_cents,
        })
    }

    /// Calculates the edge for a fill.
    ///
    /// # Arguments
    /// * `fill_price_cents` - Price at which the order was filled
    /// * `theo_cents` - Theoretical value the caller attributes to the fill
    ///   (the engine supplies the quote-time theo as a within-one-tick
    ///   approximation of the fill-time value)
    /// * `is_buy` - True for the buy leg, false for the sell leg
    ///
    /// # Returns
    /// Edge in cents per contract (positive = favorable, negative = adverse).
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
        Self::new(
            OptionPricer::default(),
            DEFAULT_BASE_SPREAD_BPS,
            DEFAULT_BASE_SIZE,
        )
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

        let quote = quoter
            .generate_quote(&input)
            .expect("a finite theo must yield a quote");

        assert!(quote.bid_price < quote.ask_price);
        assert!(quote.bid_price >= 1);
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

        let neutral = quoter
            .generate_quote(&neutral_input)
            .expect("neutral input must yield a quote");
        let bullish = quoter
            .generate_quote(&bullish_input)
            .expect("bullish input must yield a quote");

        // Bullish skew should have a tighter (>=) bid for calls. At this default
        // ATM/spread the integer `skew_adjustment` truncates to 0, so the bid is
        // unchanged here; `test_directional_skew_is_symmetric_parallel_shift`
        // exercises the non-truncated case where the bid strictly moves.
        assert!(bullish.bid_price >= neutral.bid_price);
    }

    #[test]
    fn test_directional_skew_is_symmetric_parallel_shift() {
        // Regression for issue #50: directional skew must be a same-signed,
        // symmetric (parallel) shift of bid AND ask, not an asymmetric
        // spread-widening. Use a large theo and a wide spread multiplier so the
        // integer `skew_adjustment` is comfortably >= 1 and is NOT truncated to
        // 0 (the truncation that masked the original sign bug at ATM/default
        // spread).
        let quoter = Quoter::default();
        let exp = ExpirationDate::Days(Positive::THIRTY);

        let call_neutral = QuoteInput {
            spot_cents: 1_000_000,   // $10,000 spot
            strike_cents: 1_000_000, // $10,000 strike (ATM, large theo)
            expiration: &exp,
            style: OptionStyle::Call,
            spread_multiplier: 10.0, // widen so half_spread_cents is large
            size_scalar: 1.0,
            directional_skew: 0.0,
            iv: Some(0.50),
        };
        let call_bullish = QuoteInput {
            directional_skew: 0.5,
            ..call_neutral.clone()
        };
        let call_bearish = QuoteInput {
            directional_skew: -0.5,
            ..call_neutral.clone()
        };

        let neutral = quoter
            .generate_quote(&call_neutral)
            .expect("neutral call must yield a quote");
        let bullish = quoter
            .generate_quote(&call_bullish)
            .expect("bullish call must yield a quote");
        let bearish = quoter
            .generate_quote(&call_bearish)
            .expect("bearish call must yield a quote");

        // Bullish call: BOTH bid and ask rise, by the SAME signed amount.
        let bid_delta = bullish.bid_price as i128 - neutral.bid_price as i128;
        let ask_delta = bullish.ask_price as i128 - neutral.ask_price as i128;
        assert!(
            bid_delta >= 1,
            "bullish skew must strictly raise the call bid (tighter bid); \
             skew_adjustment must be >= 1, got {bid_delta}"
        );
        assert!(
            ask_delta >= 1,
            "bullish skew must strictly raise the call ask (wider ask); got {ask_delta}"
        );
        assert_eq!(
            bid_delta, ask_delta,
            "bullish skew must shift bid and ask by the same signed amount"
        );

        // Bearish call: BOTH bid and ask fall, by the SAME signed amount, and
        // exactly mirror the bullish move at the same magnitude.
        let bear_bid_delta = bearish.bid_price as i128 - neutral.bid_price as i128;
        let bear_ask_delta = bearish.ask_price as i128 - neutral.ask_price as i128;
        assert!(
            bear_bid_delta <= -1,
            "bearish skew must strictly lower the call bid; got {bear_bid_delta}"
        );
        assert_eq!(
            bear_bid_delta, bear_ask_delta,
            "bearish skew must shift bid and ask by the same signed amount"
        );
        assert_eq!(
            bid_delta, -bear_bid_delta,
            "bullish and bearish skew of equal magnitude must be opposite shifts"
        );

        // The spread width is preserved under a parallel shift.
        let neutral_spread = neutral.ask_price - neutral.bid_price;
        assert_eq!(
            bullish.ask_price - bullish.bid_price,
            neutral_spread,
            "skew must not change the quoted spread width"
        );
        assert_eq!(
            bearish.ask_price - bearish.bid_price,
            neutral_spread,
            "skew must not change the quoted spread width"
        );

        // Mirror for a put: its value moves opposite to the underlying, so a
        // bullish skew LOWERS both the put bid and ask by the same amount.
        let put_neutral = QuoteInput {
            style: OptionStyle::Put,
            ..call_neutral.clone()
        };
        let put_bullish = QuoteInput {
            directional_skew: 0.5,
            ..put_neutral.clone()
        };
        let put_n = quoter
            .generate_quote(&put_neutral)
            .expect("neutral put must yield a quote");
        let put_b = quoter
            .generate_quote(&put_bullish)
            .expect("bullish put must yield a quote");

        let put_bid_delta = put_b.bid_price as i128 - put_n.bid_price as i128;
        let put_ask_delta = put_b.ask_price as i128 - put_n.ask_price as i128;
        assert!(
            put_bid_delta <= -1,
            "bullish skew must strictly lower the put bid; got {put_bid_delta}"
        );
        assert_eq!(
            put_bid_delta, put_ask_delta,
            "bullish skew on a put must shift bid and ask by the same signed amount"
        );
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

    #[test]
    fn test_generate_quote_skips_non_finite_theo() {
        let quoter = Quoter::default();
        let exp = ExpirationDate::Days(Positive::THIRTY);
        let pricer = OptionPricer::default();

        // An infinite implied volatility drives the Black-Scholes approximation
        // to a NaN theoretical value (Inf / Inf). Confirm the pricer really does
        // produce a non-finite value, then assert the quoter refuses to quote.
        for bad_iv in [f64::INFINITY, f64::NAN] {
            let theo =
                pricer.theoretical_value(100.0, 100.0, &exp, OptionStyle::Call, Some(bad_iv));
            assert!(
                !theo.is_finite(),
                "expected a non-finite theo for iv={bad_iv}, got {theo}"
            );

            let input = QuoteInput {
                spot_cents: 10000,
                strike_cents: 10000,
                expiration: &exp,
                style: OptionStyle::Call,
                spread_multiplier: 1.0,
                size_scalar: 1.0,
                directional_skew: 0.0,
                iv: Some(bad_iv),
            };

            assert!(
                quoter.generate_quote(&input).is_none(),
                "a non-finite theo must skip quoting (iv={bad_iv})"
            );
        }
    }

    #[test]
    fn test_non_finite_theo_never_becomes_cents() {
        // Guard test: a non-finite theoretical value must never be cast into a
        // cents number. The quoter returns `None`, so no `QuoteParams` (and thus
        // no bid/ask cents) is ever derived from the non-finite value. This
        // protects against the silent `NaN -> 0` / `+Inf -> u64::MAX -> -1` casts.
        let quoter = Quoter::default();
        let exp = ExpirationDate::Days(Positive::THIRTY);

        let input = QuoteInput {
            spot_cents: 10000,
            strike_cents: 10000,
            expiration: &exp,
            style: OptionStyle::Put,
            spread_multiplier: 1.0,
            size_scalar: 1.0,
            directional_skew: 0.0,
            iv: Some(f64::INFINITY),
        };

        match quoter.generate_quote(&input) {
            None => {}
            Some(params) => panic!(
                "non-finite theo leaked into cents: bid={} ask={}",
                params.bid_price, params.ask_price
            ),
        }
    }

    #[test]
    fn test_generate_quote_finite_input_is_valid() {
        // A normal, finite input still produces a valid two-sided quote.
        let quoter = Quoter::default();
        let exp = ExpirationDate::Days(Positive::THIRTY);

        let input = QuoteInput {
            spot_cents: 10000,
            strike_cents: 10000,
            expiration: &exp,
            style: OptionStyle::Call,
            spread_multiplier: 1.0,
            size_scalar: 1.0,
            directional_skew: 0.0,
            iv: Some(0.20),
        };

        let quote = quoter
            .generate_quote(&input)
            .expect("a finite input must yield a quote");
        assert!(quote.ask_price > quote.bid_price);
        assert!(quote.bid_price >= 1);
        assert!(quote.bid_size >= 1);
        assert!(quote.ask_size >= 1);
    }
}

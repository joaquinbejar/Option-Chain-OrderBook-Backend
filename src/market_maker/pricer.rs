//! Option pricing utilities for market making.

use optionstratlib::{ExpirationDate, OptionStyle};

/// Simple option pricer for market making purposes.
///
/// Uses Black-Scholes approximation for theoretical values.
pub struct OptionPricer {
    /// Risk-free rate (annualized).
    risk_free_rate: f64,
    /// Default implied volatility if not provided.
    default_iv: f64,
}

impl OptionPricer {
    /// Creates a new option pricer.
    ///
    /// # Arguments
    /// * `risk_free_rate` - Annualized risk-free rate (e.g., 0.05 for 5%)
    /// * `default_iv` - Default implied volatility (e.g., 0.30 for 30%)
    #[must_use]
    pub fn new(risk_free_rate: f64, default_iv: f64) -> Self {
        Self {
            risk_free_rate,
            default_iv,
        }
    }

    /// Calculates the theoretical value of an option.
    ///
    /// # Arguments
    /// * `spot` - Current underlying price
    /// * `strike` - Option strike price
    /// * `expiration` - Time to expiration
    /// * `style` - Call or Put
    /// * `iv` - Optional implied volatility override
    ///
    /// # Returns
    /// Theoretical option value in the same units as spot/strike.
    #[must_use]
    pub fn theoretical_value(
        &self,
        spot: f64,
        strike: f64,
        expiration: &ExpirationDate,
        style: OptionStyle,
        iv: Option<f64>,
    ) -> f64 {
        let sigma = iv.unwrap_or(self.default_iv);
        let t = self.time_to_expiry(expiration);

        if t <= 0.0 {
            // Expired option - return intrinsic value
            return match style {
                OptionStyle::Call => (spot - strike).max(0.0),
                OptionStyle::Put => (strike - spot).max(0.0),
            };
        }

        // Black-Scholes formula
        let d1 = ((spot / strike).ln() + (self.risk_free_rate + sigma * sigma / 2.0) * t)
            / (sigma * t.sqrt());
        let d2 = d1 - sigma * t.sqrt();

        let discount = (-self.risk_free_rate * t).exp();

        match style {
            OptionStyle::Call => spot * Self::norm_cdf(d1) - strike * discount * Self::norm_cdf(d2),
            OptionStyle::Put => {
                strike * discount * Self::norm_cdf(-d2) - spot * Self::norm_cdf(-d1)
            }
        }
    }

    /// Calculates delta for an option.
    #[must_use]
    pub fn delta(
        &self,
        spot: f64,
        strike: f64,
        expiration: &ExpirationDate,
        style: OptionStyle,
        iv: Option<f64>,
    ) -> f64 {
        let sigma = iv.unwrap_or(self.default_iv);
        let t = self.time_to_expiry(expiration);

        if t <= 0.0 {
            return match style {
                OptionStyle::Call => {
                    if spot > strike {
                        1.0
                    } else {
                        0.0
                    }
                }
                OptionStyle::Put => {
                    if spot < strike {
                        -1.0
                    } else {
                        0.0
                    }
                }
            };
        }

        let d1 = ((spot / strike).ln() + (self.risk_free_rate + sigma * sigma / 2.0) * t)
            / (sigma * t.sqrt());

        match style {
            OptionStyle::Call => Self::norm_cdf(d1),
            OptionStyle::Put => Self::norm_cdf(d1) - 1.0,
        }
    }

    /// Calculates gamma for an option.
    #[must_use]
    pub fn gamma(
        &self,
        spot: f64,
        strike: f64,
        expiration: &ExpirationDate,
        iv: Option<f64>,
    ) -> f64 {
        let sigma = iv.unwrap_or(self.default_iv);
        let t = self.time_to_expiry(expiration);

        if t <= 0.0 {
            return 0.0;
        }

        let d1 = ((spot / strike).ln() + (self.risk_free_rate + sigma * sigma / 2.0) * t)
            / (sigma * t.sqrt());

        Self::norm_pdf(d1) / (spot * sigma * t.sqrt())
    }

    /// Calculates vega for an option.
    #[must_use]
    pub fn vega(
        &self,
        spot: f64,
        strike: f64,
        expiration: &ExpirationDate,
        iv: Option<f64>,
    ) -> f64 {
        let sigma = iv.unwrap_or(self.default_iv);
        let t = self.time_to_expiry(expiration);

        if t <= 0.0 {
            return 0.0;
        }

        let d1 = ((spot / strike).ln() + (self.risk_free_rate + sigma * sigma / 2.0) * t)
            / (sigma * t.sqrt());

        spot * Self::norm_pdf(d1) * t.sqrt() / 100.0 // Per 1% vol change
    }

    /// Calculates theta for an option (daily decay).
    #[must_use]
    pub fn theta(
        &self,
        spot: f64,
        strike: f64,
        expiration: &ExpirationDate,
        style: OptionStyle,
        iv: Option<f64>,
    ) -> f64 {
        let sigma = iv.unwrap_or(self.default_iv);
        let t = self.time_to_expiry(expiration);

        if t <= 0.0 {
            return 0.0;
        }

        let d1 = ((spot / strike).ln() + (self.risk_free_rate + sigma * sigma / 2.0) * t)
            / (sigma * t.sqrt());
        let d2 = d1 - sigma * t.sqrt();

        let discount = (-self.risk_free_rate * t).exp();
        let term1 = -spot * Self::norm_pdf(d1) * sigma / (2.0 * t.sqrt());

        let theta = match style {
            OptionStyle::Call => {
                term1 - self.risk_free_rate * strike * discount * Self::norm_cdf(d2)
            }
            OptionStyle::Put => {
                term1 + self.risk_free_rate * strike * discount * Self::norm_cdf(-d2)
            }
        };

        theta / 365.0 // Daily theta
    }

    /// Converts expiration to time in years.
    fn time_to_expiry(&self, expiration: &ExpirationDate) -> f64 {
        match expiration {
            ExpirationDate::Days(days) => days.to_f64() / 365.0,
            ExpirationDate::DateTime(dt) => {
                let now = chrono::Utc::now();
                let duration = *dt - now;
                duration.num_seconds() as f64 / (365.0 * 24.0 * 3600.0)
            }
        }
    }

    /// Standard normal CDF approximation.
    fn norm_cdf(x: f64) -> f64 {
        0.5 * (1.0 + Self::erf(x / std::f64::consts::SQRT_2))
    }

    /// Standard normal PDF.
    fn norm_pdf(x: f64) -> f64 {
        (-x * x / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt()
    }

    /// Error function approximation.
    fn erf(x: f64) -> f64 {
        let a1 = 0.254829592;
        let a2 = -0.284496736;
        let a3 = 1.421413741;
        let a4 = -1.453152027;
        let a5 = 1.061405429;
        let p = 0.3275911;

        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = x.abs();

        let t = 1.0 / (1.0 + p * x);
        let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

        sign * y
    }
}

impl Default for OptionPricer {
    fn default() -> Self {
        Self::new(0.05, 0.30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optionstratlib::pos;

    #[test]
    fn test_call_price() {
        let pricer = OptionPricer::default();
        let exp = ExpirationDate::Days(pos!(30.0));
        let price = pricer.theoretical_value(100.0, 100.0, &exp, OptionStyle::Call, Some(0.20));
        assert!(price > 0.0);
        assert!(price < 10.0); // ATM 30-day call should be reasonable
    }

    #[test]
    fn test_put_price() {
        let pricer = OptionPricer::default();
        let exp = ExpirationDate::Days(pos!(30.0));
        let price = pricer.theoretical_value(100.0, 100.0, &exp, OptionStyle::Put, Some(0.20));
        assert!(price > 0.0);
    }

    #[test]
    fn test_delta() {
        let pricer = OptionPricer::default();
        let exp = ExpirationDate::Days(pos!(30.0));

        let call_delta = pricer.delta(100.0, 100.0, &exp, OptionStyle::Call, Some(0.20));
        assert!(call_delta > 0.4 && call_delta < 0.6); // ATM call delta ~0.5

        let put_delta = pricer.delta(100.0, 100.0, &exp, OptionStyle::Put, Some(0.20));
        assert!(put_delta > -0.6 && put_delta < -0.4); // ATM put delta ~-0.5
    }
}

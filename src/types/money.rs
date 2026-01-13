//! Monetary value representation with precise decimal arithmetic.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Sub};
use std::str::FromStr;

/// Represents a monetary value with precise decimal arithmetic.
///
/// Uses [`Decimal`] internally to avoid floating-point precision issues
/// that are critical in financial calculations.
///
/// # Examples
///
/// ```
/// use cloud_billing_sim::types::Money;
/// use std::str::FromStr;
///
/// let price = Money::from_str("0.023").unwrap(); // $0.023
/// let total = price * 1000; // $23.00
/// assert_eq!(total, Money::from_str("23").unwrap());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(Decimal);

impl Money {
    /// Zero cost.
    pub const ZERO: Self = Self(Decimal::ZERO);

    /// Creates a new `Money` from a decimal value.
    #[must_use]
    pub const fn new(value: Decimal) -> Self {
        Self(value)
    }

    /// Creates a `Money` value from dollars and cents.
    ///
    /// # Examples
    ///
    /// ```
    /// use cloud_billing_sim::types::Money;
    ///
    /// let five_dollars = Money::from_dollars(5, 0);
    /// let five_fifty = Money::from_dollars(5, 50);
    /// ```
    #[must_use]
    pub fn from_dollars(dollars: u64, cents: u32) -> Self {
        let cents_decimal = Decimal::new(i64::from(cents), 2);
        let dollars_decimal = Decimal::from(dollars);
        Self(dollars_decimal + cents_decimal)
    }

    /// Returns the underlying decimal value.
    #[must_use]
    pub const fn as_decimal(&self) -> Decimal {
        self.0
    }

    /// Returns true if this amount is zero.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Rounds to a specified number of decimal places.
    #[must_use]
    pub fn round_dp(&self, dp: u32) -> Self {
        Self(self.0.round_dp(dp))
    }
}

impl Default for Money {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:.4}", self.0)
    }
}

impl Add for Money {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Money {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul<u64> for Money {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self::Output {
        Self(self.0 * Decimal::from(rhs))
    }
}

impl Mul<Decimal> for Money {
    type Output = Self;

    fn mul(self, rhs: Decimal) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, m| acc + m)
    }
}

impl FromStr for Money {
    type Err = rust_decimal::Error;

    /// Creates a `Money` value from a fractional dollar amount string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string cannot be parsed as a decimal.
    ///
    /// # Examples
    ///
    /// ```
    /// use cloud_billing_sim::types::Money;
    /// use std::str::FromStr;
    ///
    /// let price = Money::from_str("0.023").unwrap();
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Decimal>().map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_dollars_creates_correct_value() {
        let m = Money::from_dollars(5, 50);
        assert_eq!(m.as_decimal(), Decimal::new(550, 2));
    }

    #[test]
    fn addition_works() {
        let a = Money::from_dollars(1, 50);
        let b = Money::from_dollars(2, 25);
        assert_eq!(a + b, Money::from_dollars(3, 75));
    }

    #[test]
    fn multiplication_by_quantity() {
        let price = Money::from_str("0.023").ok();
        let total = price.map(|p| p * 1000);
        assert_eq!(total, Money::from_str("23").ok());
    }
}

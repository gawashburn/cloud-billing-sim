//! Monetary value representation with precise decimal arithmetic.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul};
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

// ============================================================================
// Kani Formal Verification
// ============================================================================

#[cfg(kani)]
impl kani::Arbitrary for Money {
    fn any() -> Self {
        // Generate bounded money values (0 to $1,000,000.00)
        // Using cents to avoid decimal complexity in verification
        let cents: u64 = kani::any();
        kani::assume(cents <= 100_000_000); // Max $1M
        Self::from_dollars(cents / 100, (cents % 100) as u32)
    }
}

#[cfg(kani)]
mod verification {
    use super::*;

    /// Verifies that Money addition is commutative: a + b == b + a
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_addition_commutative() {
        let a: Money = kani::any();
        let b: Money = kani::any();

        assert!(a + b == b + a, "Addition must be commutative");
    }

    /// Verifies that adding zero doesn't change the value: a + ZERO == a
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_zero_identity() {
        let a: Money = kani::any();

        assert!(a + Money::ZERO == a, "Zero must be additive identity");
        assert!(
            Money::ZERO + a == a,
            "Zero must be additive identity (left)"
        );
    }

    /// Verifies that Money::ZERO.is_zero() returns true
    #[kani::proof]
    fn verify_zero_is_zero() {
        assert!(Money::ZERO.is_zero(), "ZERO constant must be zero");
    }

    /// Verifies that multiplying by zero yields zero
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_multiply_by_zero() {
        let a: Money = kani::any();

        assert!((a * 0u64).is_zero(), "Multiplying by zero must yield zero");
    }

    /// Verifies that multiplying by one doesn't change the value
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_multiply_by_one() {
        let a: Money = kani::any();

        assert!(a * 1u64 == a, "Multiplying by one must be identity");
    }

    /// Verifies that from_dollars creates consistent values
    #[kani::proof]
    fn verify_from_dollars_consistency() {
        let dollars: u64 = kani::any();
        let cents: u32 = kani::any();

        // Bound inputs for tractable verification
        kani::assume(dollars <= 10_000);
        kani::assume(cents < 100);

        let m = Money::from_dollars(dollars, cents);
        let expected = Decimal::from(dollars) + Decimal::new(i64::from(cents), 2);

        assert!(
            m.as_decimal() == expected,
            "from_dollars must create expected decimal value"
        );
    }

    /// Verifies non-zero money is not zero
    #[kani::proof]
    fn verify_nonzero_is_not_zero() {
        let dollars: u64 = kani::any();
        let cents: u32 = kani::any();

        kani::assume(dollars <= 10_000);
        kani::assume(cents < 100);
        kani::assume(dollars > 0 || cents > 0);

        let m = Money::from_dollars(dollars, cents);
        assert!(!m.is_zero(), "Non-zero money must not be zero");
    }

    /// Verifies ordering consistency with underlying decimal
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_ordering_consistent() {
        let a: Money = kani::any();
        let b: Money = kani::any();

        // If a <= b and b <= a, then a == b
        if a <= b && b <= a {
            assert!(a == b, "Ordering must be antisymmetric");
        }

        // Ordering matches decimal ordering
        assert!(
            a.cmp(&b) == a.as_decimal().cmp(&b.as_decimal()),
            "Money ordering must match decimal ordering"
        );
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

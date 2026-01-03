//! Storage size representation.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// Represents a storage size in bytes.
///
/// Provides conversions to/from common units (KB, MB, GB, TB) using
/// binary prefixes (1 KB = 1024 bytes).
///
/// # Examples
///
/// ```
/// use cloud_billing_sim::types::Bytes;
///
/// let size = Bytes::from_gb(5);
/// assert_eq!(size.as_bytes(), 5 * 1024 * 1024 * 1024);
/// ```
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Bytes(u64);

impl Bytes {
    /// Zero bytes.
    pub const ZERO: Self = Self(0);

    /// One kilobyte (1024 bytes).
    pub const KB: u64 = 1024;
    /// One megabyte (1024 KB).
    pub const MB: u64 = 1024 * Self::KB;
    /// One gigabyte (1024 MB).
    pub const GB: u64 = 1024 * Self::MB;
    /// One terabyte (1024 GB).
    pub const TB: u64 = 1024 * Self::GB;

    /// Creates a new `Bytes` from a raw byte count.
    #[must_use]
    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    /// Creates `Bytes` from kilobytes.
    #[must_use]
    pub const fn from_kb(kb: u64) -> Self {
        Self(kb * Self::KB)
    }

    /// Creates `Bytes` from megabytes.
    #[must_use]
    pub const fn from_mb(mb: u64) -> Self {
        Self(mb * Self::MB)
    }

    /// Creates `Bytes` from gigabytes.
    #[must_use]
    pub const fn from_gb(gb: u64) -> Self {
        Self(gb * Self::GB)
    }

    /// Creates `Bytes` from terabytes.
    #[must_use]
    pub const fn from_tb(tb: u64) -> Self {
        Self(tb * Self::TB)
    }

    /// Returns the raw byte count.
    #[must_use]
    pub const fn as_bytes(&self) -> u64 {
        self.0
    }

    /// Returns the size in gigabytes as a decimal for pricing calculations.
    ///
    /// Cloud providers typically price storage per GB-month, so this
    /// provides precise fractional GB values.
    #[must_use]
    pub fn as_gb_decimal(&self) -> Decimal {
        Decimal::from(self.0) / Decimal::from(Self::GB)
    }

    /// Returns the size in terabytes as a decimal.
    #[must_use]
    pub fn as_tb_decimal(&self) -> Decimal {
        Decimal::from(self.0) / Decimal::from(Self::TB)
    }

    /// Returns true if this is zero bytes.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Returns the maximum of this value and another.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        if self.0 > other.0 {
            self
        } else {
            other
        }
    }

    /// Saturating subtraction.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 >= Self::TB {
            write!(f, "{:.2} TB", self.as_tb_decimal())
        } else if self.0 >= Self::GB {
            write!(f, "{:.2} GB", self.as_gb_decimal())
        } else if self.0 >= Self::MB {
            write!(
                f,
                "{:.2} MB",
                Decimal::from(self.0) / Decimal::from(Self::MB)
            )
        } else if self.0 >= Self::KB {
            write!(
                f,
                "{:.2} KB",
                Decimal::from(self.0) / Decimal::from(Self::KB)
            )
        } else {
            write!(f, "{} B", self.0)
        }
    }
}

impl Add for Bytes {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Bytes {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Bytes {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Bytes {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

// ============================================================================
// Kani Formal Verification
// ============================================================================

#[cfg(kani)]
impl kani::Arbitrary for Bytes {
    fn any() -> Self {
        // Generate bounded byte values (0 to 1 PB)
        // Using smaller range for tractable verification
        let bytes: u64 = kani::any();
        kani::assume(bytes <= 1_000_000_000_000_000); // Max 1 PB
        Self::new(bytes)
    }
}

#[cfg(kani)]
mod verification {
    use super::*;

    /// Verifies that Bytes addition is commutative: a + b == b + a
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_addition_commutative() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();

        // Bound to prevent overflow
        kani::assume(a <= 1_000_000_000);
        kani::assume(b <= 1_000_000_000);

        let bytes_a = Bytes::new(a);
        let bytes_b = Bytes::new(b);

        assert!(
            bytes_a + bytes_b == bytes_b + bytes_a,
            "Addition must be commutative"
        );
    }

    /// Verifies that adding zero doesn't change the value
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_zero_identity() {
        let a: Bytes = kani::any();

        assert!(a + Bytes::ZERO == a, "Zero must be additive identity");
        assert!(
            Bytes::ZERO + a == a,
            "Zero must be additive identity (left)"
        );
    }

    /// Verifies that Bytes::ZERO.is_zero() returns true
    #[kani::proof]
    fn verify_zero_is_zero() {
        assert!(Bytes::ZERO.is_zero(), "ZERO constant must be zero");
    }

    /// Verifies non-zero bytes is not zero
    #[kani::proof]
    fn verify_nonzero_is_not_zero() {
        let bytes: u64 = kani::any();
        kani::assume(bytes > 0);
        kani::assume(bytes <= 1_000_000_000_000);

        let b = Bytes::new(bytes);
        assert!(!b.is_zero(), "Non-zero bytes must not be zero");
    }

    /// Verifies saturating_sub never underflows
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_saturating_sub_no_underflow() {
        let a: Bytes = kani::any();
        let b: Bytes = kani::any();

        let result = a.saturating_sub(b);

        // Result is always <= a (never negative)
        assert!(result <= a, "Saturating sub must not exceed original");

        // If a >= b, result == a - b
        if a >= b {
            assert!(
                result.as_bytes() == a.as_bytes() - b.as_bytes(),
                "Saturating sub must equal normal sub when a >= b"
            );
        } else {
            // If a < b, result == 0
            assert!(result.is_zero(), "Saturating sub must be zero when a < b");
        }
    }

    /// Verifies max returns the greater value
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_max_returns_greater() {
        let a: Bytes = kani::any();
        let b: Bytes = kani::any();

        let max = a.max(b);

        assert!(max >= a, "max must be >= a");
        assert!(max >= b, "max must be >= b");
        assert!(max == a || max == b, "max must equal a or b");
    }

    /// Verifies from_kb creates correct byte count
    #[kani::proof]
    fn verify_from_kb_correct() {
        let kb: u64 = kani::any();
        kani::assume(kb <= 1_000_000); // Prevent overflow

        let b = Bytes::from_kb(kb);
        assert!(
            b.as_bytes() == kb * Bytes::KB,
            "from_kb must multiply by 1024"
        );
    }

    /// Verifies from_mb creates correct byte count
    #[kani::proof]
    fn verify_from_mb_correct() {
        let mb: u64 = kani::any();
        kani::assume(mb <= 1_000); // Prevent overflow

        let b = Bytes::from_mb(mb);
        assert!(
            b.as_bytes() == mb * Bytes::MB,
            "from_mb must multiply by 1024^2"
        );
    }

    /// Verifies from_gb creates correct byte count
    #[kani::proof]
    fn verify_from_gb_correct() {
        let gb: u64 = kani::any();
        kani::assume(gb <= 1_000); // Prevent overflow

        let b = Bytes::from_gb(gb);
        assert!(
            b.as_bytes() == gb * Bytes::GB,
            "from_gb must multiply by 1024^3"
        );
    }

    /// Verifies ordering consistency with underlying u64
    #[kani::proof]
    #[kani::unwind(2)]
    fn verify_ordering_consistent() {
        let a: Bytes = kani::any();
        let b: Bytes = kani::any();

        // Ordering matches underlying u64 ordering
        assert!(
            a.cmp(&b) == a.as_bytes().cmp(&b.as_bytes()),
            "Bytes ordering must match u64 ordering"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_conversions() {
        assert_eq!(Bytes::from_kb(1).as_bytes(), 1024);
        assert_eq!(Bytes::from_mb(1).as_bytes(), 1024 * 1024);
        assert_eq!(Bytes::from_gb(1).as_bytes(), 1024 * 1024 * 1024);
        assert_eq!(Bytes::from_tb(1).as_bytes(), 1024 * 1024 * 1024 * 1024);
    }

    #[test]
    fn display_formats_appropriately() {
        assert_eq!(format!("{}", Bytes::new(500)), "500 B");
        assert_eq!(format!("{}", Bytes::from_kb(1)), "1.00 KB");
        assert_eq!(format!("{}", Bytes::from_gb(5)), "5.00 GB");
    }
}

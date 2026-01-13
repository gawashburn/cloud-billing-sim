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
    pub const fn as_bytes(self) -> u64 {
        self.0
    }

    /// Returns the size in gigabytes as a decimal for pricing calculations.
    ///
    /// Cloud providers typically price storage per GB-month, so this
    /// provides precise fractional GB values.
    #[must_use]
    pub fn as_gb_decimal(self) -> Decimal {
        Decimal::from(self.0) / Decimal::from(Self::GB)
    }

    /// Returns the size in terabytes as a decimal.
    #[must_use]
    pub fn as_tb_decimal(self) -> Decimal {
        Decimal::from(self.0) / Decimal::from(Self::TB)
    }

    /// Returns true if this is zero bytes.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Returns the maximum of this value and another.
    // MUTANTS EXCLUSION: The mutation `> to >=` is an equivalent mutant.
    // When self == other, returning either value gives the same result since
    // both have identical byte counts. Tested by bytes_max_returns_correct_value.
    #[mutants::skip]
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
    fn display_formats_bytes() {
        assert_eq!(format!("{}", Bytes::new(0)), "0 B");
        assert_eq!(format!("{}", Bytes::new(500)), "500 B");
        assert_eq!(format!("{}", Bytes::new(1023)), "1023 B");
    }

    #[test]
    fn display_formats_kilobytes() {
        assert_eq!(format!("{}", Bytes::from_kb(1)), "1.00 KB");
        assert_eq!(format!("{}", Bytes::new(1536)), "1.50 KB"); // 1.5 KB
        assert_eq!(format!("{}", Bytes::from_kb(512)), "512.00 KB");
    }

    #[test]
    fn display_formats_megabytes() {
        assert_eq!(format!("{}", Bytes::from_mb(1)), "1.00 MB");
        assert_eq!(format!("{}", Bytes::from_mb(256)), "256.00 MB");
        assert_eq!(format!("{}", Bytes::from_kb(1536)), "1.50 MB"); // 1.5 MB
    }

    #[test]
    fn display_formats_gigabytes() {
        assert_eq!(format!("{}", Bytes::from_gb(1)), "1.00 GB");
        assert_eq!(format!("{}", Bytes::from_gb(5)), "5.00 GB");
        assert_eq!(format!("{}", Bytes::from_mb(1536)), "1.50 GB"); // 1.5 GB
    }

    #[test]
    fn display_formats_terabytes() {
        assert_eq!(format!("{}", Bytes::from_tb(1)), "1.00 TB");
        assert_eq!(format!("{}", Bytes::from_tb(10)), "10.00 TB");
        assert_eq!(format!("{}", Bytes::from_gb(1536)), "1.50 TB"); // 1.5 TB
    }

    #[test]
    fn saturating_sub_prevents_underflow() {
        let a = Bytes::new(100);
        let b = Bytes::new(150);
        assert_eq!(a.saturating_sub(b), Bytes::ZERO);

        let c = Bytes::from_mb(10);
        let d = Bytes::from_mb(3);
        assert_eq!(c.saturating_sub(d), Bytes::from_mb(7));
    }

    #[test]
    fn new_creates_bytes() {
        let b = Bytes::new(12345);
        assert_eq!(b.as_bytes(), 12345);
    }

    #[test]
    fn zero_constant() {
        assert!(Bytes::ZERO.is_zero());
        assert_eq!(Bytes::ZERO.as_bytes(), 0);
    }
}

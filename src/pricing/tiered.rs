//! Tiered pricing support.

use crate::types::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

/// A price tier with an optional upper bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceTier {
    /// Upper bound in GB (exclusive). `None` means unlimited.
    #[serde(default)]
    pub up_to_gb: Option<u64>,
    /// Price per GB for this tier.
    #[serde(deserialize_with = "deserialize_money")]
    pub price: Money,
}

fn deserialize_money<'de, D>(deserializer: D) -> Result<Money, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Money::from_str(&s).map_err(serde::de::Error::custom)
}

/// Represents a price that may be flat or tiered by volume.
///
/// Cloud providers often use tiered pricing where the per-unit cost
/// decreases as usage increases.
///
/// # Examples
///
/// Flat pricing:
/// ```
/// use cloud_billing_sim::pricing::TieredPrice;
/// use cloud_billing_sim::types::Money;
/// use rust_decimal::Decimal;
/// use std::str::FromStr;
///
/// let flat = TieredPrice::flat(Money::from_str("0.023").unwrap());
/// let cost = flat.calculate_cost(Decimal::from(1000)); // 1000 GB
/// ```
///
/// Tiered pricing:
/// ```toml
/// storage_price_per_gb_month = [
///     { up_to_gb = 50000, price = "0.023" },
///     { up_to_gb = 500000, price = "0.022" },
///     { price = "0.021" }
/// ]
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum TieredPrice {
    /// A single flat price per unit.
    Flat(Money),
    /// Multiple tiers with decreasing prices for higher usage.
    Tiered(Vec<PriceTier>),
}

impl TieredPrice {
    /// Creates a flat (non-tiered) price.
    #[must_use]
    pub const fn flat(price: Money) -> Self {
        Self::Flat(price)
    }

    /// Creates a tiered price from a list of tiers.
    #[must_use]
    pub const fn tiered(tiers: Vec<PriceTier>) -> Self {
        Self::Tiered(tiers)
    }

    /// Calculates the total cost for a given quantity in GB.
    ///
    /// For tiered pricing, this correctly applies each tier's price
    /// to the portion of usage within that tier.
    #[must_use]
    pub fn calculate_cost(&self, gb: Decimal) -> Money {
        match self {
            Self::Flat(price) => *price * gb,
            Self::Tiered(tiers) => {
                let mut remaining = gb;
                let mut total = Money::ZERO;
                let mut prev_threshold = Decimal::ZERO;

                for tier in tiers {
                    if remaining.is_zero() {
                        break;
                    }

                    let tier_limit = tier.up_to_gb.map_or(Decimal::MAX, Decimal::from);
                    let tier_size = tier_limit - prev_threshold;
                    let usage_in_tier = remaining.min(tier_size);

                    total += tier.price * usage_in_tier;
                    remaining -= usage_in_tier;
                    prev_threshold = tier_limit;
                }

                total
            }
        }
    }

    /// Returns the price for the first unit (used for simple lookups).
    #[must_use]
    pub fn base_price(&self) -> Money {
        match self {
            Self::Flat(price) => *price,
            Self::Tiered(tiers) => tiers.first().map_or(Money::ZERO, |t| t.price),
        }
    }
}

impl<'de> Deserialize<'de> for TieredPrice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawTieredPrice {
            Flat(String),
            Tiered(Vec<PriceTier>),
        }

        match RawTieredPrice::deserialize(deserializer)? {
            RawTieredPrice::Flat(s) => Money::from_str(&s)
                .map(TieredPrice::Flat)
                .map_err(D::Error::custom),
            RawTieredPrice::Tiered(tiers) => Ok(Self::Tiered(tiers)),
        }
    }
}

impl Default for TieredPrice {
    fn default() -> Self {
        Self::Flat(Money::ZERO)
    }
}

// ============================================================================
// Kani Formal Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;

    /// Verifies that zero GB always costs zero for flat pricing
    #[kani::proof]
    fn verify_zero_gb_costs_zero_flat() {
        let price: Money = kani::any();
        let tiered = TieredPrice::flat(price);

        let cost = tiered.calculate_cost(Decimal::ZERO);
        assert!(cost.is_zero(), "Zero GB must cost zero");
    }

    /// Verifies that flat price cost equals price * quantity
    #[kani::proof]
    fn verify_flat_price_multiplicative() {
        // Use bounded values for tractable verification
        let dollars: u64 = kani::any();
        let cents: u32 = kani::any();
        let gb: u64 = kani::any();

        kani::assume(dollars <= 100);
        kani::assume(cents < 100);
        kani::assume(gb <= 1000);

        let price = Money::from_dollars(dollars, cents);
        let tiered = TieredPrice::flat(price);

        let cost = tiered.calculate_cost(Decimal::from(gb));
        let expected = price * Decimal::from(gb);

        assert!(cost == expected, "Flat price cost must equal price * quantity");
    }

    /// Verifies that cost is non-negative for flat pricing
    #[kani::proof]
    fn verify_cost_non_negative() {
        let price: Money = kani::any();
        let gb: u64 = kani::any();
        kani::assume(gb <= 10_000);

        let tiered = TieredPrice::flat(price);
        let cost = tiered.calculate_cost(Decimal::from(gb));

        assert!(cost >= Money::ZERO, "Cost must be non-negative");
    }

    /// Verifies base_price returns the flat price for flat pricing
    #[kani::proof]
    fn verify_base_price_flat() {
        let price: Money = kani::any();
        let tiered = TieredPrice::flat(price);

        assert!(
            tiered.base_price() == price,
            "base_price must return flat price"
        );
    }

    /// Verifies default TieredPrice is flat zero
    #[kani::proof]
    fn verify_default_is_flat_zero() {
        let default = TieredPrice::default();

        assert!(
            default.base_price().is_zero(),
            "Default base_price must be zero"
        );

        let cost = default.calculate_cost(Decimal::from(100));
        assert!(cost.is_zero(), "Default pricing must yield zero cost");
    }

    /// Verifies that flat pricing cost increases monotonically
    #[kani::proof]
    fn verify_flat_cost_monotonic() {
        let price: Money = kani::any();
        let gb1: u64 = kani::any();
        let gb2: u64 = kani::any();

        kani::assume(gb1 <= 1000);
        kani::assume(gb2 <= 1000);

        let tiered = TieredPrice::flat(price);
        let cost1 = tiered.calculate_cost(Decimal::from(gb1));
        let cost2 = tiered.calculate_cost(Decimal::from(gb2));

        if gb1 <= gb2 {
            assert!(cost1 <= cost2, "Cost must increase monotonically");
        }
    }

    /// Verifies tiered pricing with two tiers: cost is bounded by max price
    #[kani::proof]
    fn verify_tiered_bounded_by_max_price() {
        // Create a simple two-tier pricing
        let price1_dollars: u64 = kani::any();
        let price1_cents: u32 = kani::any();
        let price2_dollars: u64 = kani::any();
        let price2_cents: u32 = kani::any();
        let threshold: u64 = kani::any();
        let gb: u64 = kani::any();

        // Bound inputs
        kani::assume(price1_dollars <= 10);
        kani::assume(price1_cents < 100);
        kani::assume(price2_dollars <= 10);
        kani::assume(price2_cents < 100);
        kani::assume(threshold > 0 && threshold <= 100);
        kani::assume(gb <= 200);

        let price1 = Money::from_dollars(price1_dollars, price1_cents);
        let price2 = Money::from_dollars(price2_dollars, price2_cents);

        // Assume price1 >= price2 (typical tiered pricing)
        kani::assume(price1 >= price2);

        let tiered = TieredPrice::tiered(vec![
            PriceTier {
                up_to_gb: Some(threshold),
                price: price1,
            },
            PriceTier {
                up_to_gb: None,
                price: price2,
            },
        ]);

        let cost = tiered.calculate_cost(Decimal::from(gb));
        let max_possible = price1 * Decimal::from(gb);

        assert!(
            cost <= max_possible,
            "Tiered cost must not exceed max tier price * quantity"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_price_calculation() {
        let price = TieredPrice::flat(Money::from_str("0.023").ok().unwrap_or(Money::ZERO));
        let cost = price.calculate_cost(Decimal::from(100));
        // 100 GB * $0.023 = $2.30
        assert_eq!(cost, Money::from_str("2.30").ok().unwrap_or(Money::ZERO));
    }

    #[test]
    fn tiered_price_calculation() {
        let price = TieredPrice::tiered(vec![
            PriceTier {
                up_to_gb: Some(50),
                price: Money::from_str("0.10").ok().unwrap_or(Money::ZERO),
            },
            PriceTier {
                up_to_gb: None,
                price: Money::from_str("0.05").ok().unwrap_or(Money::ZERO),
            },
        ]);

        // 100 GB: 50 @ $0.10 + 50 @ $0.05 = $5.00 + $2.50 = $7.50
        let cost = price.calculate_cost(Decimal::from(100));
        assert_eq!(cost, Money::from_str("7.50").ok().unwrap_or(Money::ZERO));
    }

    #[test]
    fn tiered_price_within_first_tier() {
        let price = TieredPrice::tiered(vec![
            PriceTier {
                up_to_gb: Some(50),
                price: Money::from_str("0.10").ok().unwrap_or(Money::ZERO),
            },
            PriceTier {
                up_to_gb: None,
                price: Money::from_str("0.05").ok().unwrap_or(Money::ZERO),
            },
        ]);

        // 30 GB: 30 @ $0.10 = $3.00
        let cost = price.calculate_cost(Decimal::from(30));
        assert_eq!(cost, Money::from_str("3.00").ok().unwrap_or(Money::ZERO));
    }
}

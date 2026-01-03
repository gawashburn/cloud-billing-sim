//! Property-based tests for cloud billing simulator.

mod strategies;

use cloud_billing_sim::pricing::TieredPrice;
use cloud_billing_sim::types::{Bytes, Money};
use proptest::prelude::*;
use rust_decimal::Decimal;
use std::str::FromStr;

use strategies::{
    bytes_strategy, flat_price_strategy, gb_bytes_strategy, gb_quantity_strategy, money_strategy,
    nonzero_bytes_strategy, nonzero_money_strategy, small_bytes_strategy, small_money_strategy,
    tiered_price_strategy,
};

// ============================================================================
// Money Property Tests
// ============================================================================

proptest! {
    /// Property: Money addition is commutative.
    #[test]
    fn money_addition_commutative(a in small_money_strategy(), b in small_money_strategy()) {
        prop_assert_eq!(a + b, b + a);
    }

    /// Property: Money addition is associative.
    #[test]
    fn money_addition_associative(
        a in small_money_strategy(),
        b in small_money_strategy(),
        c in small_money_strategy()
    ) {
        prop_assert_eq!((a + b) + c, a + (b + c));
    }

    /// Property: Adding zero doesn't change value.
    #[test]
    fn money_add_zero_identity(a in money_strategy()) {
        prop_assert_eq!(a + Money::ZERO, a);
        prop_assert_eq!(Money::ZERO + a, a);
    }

    /// Property: Multiplying by zero yields zero.
    #[test]
    #[allow(clippy::erasing_op)] // Intentionally testing that a * 0 = 0
    fn money_multiply_by_zero(a in money_strategy()) {
        prop_assert_eq!(a * 0u64, Money::ZERO);
        prop_assert_eq!(a * Decimal::ZERO, Money::ZERO);
    }

    /// Property: Multiplying by one doesn't change value.
    #[test]
    fn money_multiply_by_one(a in money_strategy()) {
        prop_assert_eq!(a * 1u64, a);
        prop_assert_eq!(a * Decimal::ONE, a);
    }

    /// Property: from_dollars creates consistent values.
    #[test]
    fn money_from_dollars_consistent(dollars in 0u64..1_000_000u64, cents in 0u32..100u32) {
        let m = Money::from_dollars(dollars, cents);
        // Value should equal dollars + cents/100
        let expected = Decimal::from(dollars) + Decimal::new(i64::from(cents), 2);
        prop_assert_eq!(m.as_decimal(), expected);
    }

    /// Property: Money::ZERO.is_zero() is true.
    #[test]
    fn money_zero_is_zero(_x in 0..1i32) {
        prop_assert!(Money::ZERO.is_zero());
    }

    /// Property: Non-zero money is not zero.
    #[test]
    fn money_nonzero_is_not_zero(m in nonzero_money_strategy()) {
        prop_assert!(!m.is_zero());
    }

    /// Property: Money from_str roundtrips correctly.
    #[test]
    fn money_from_str_roundtrip(dollars in 0u64..1_000_000u64, cents in 0u32..100u32) {
        let m = Money::from_dollars(dollars, cents);
        let s = format!("{dollars}.{cents:02}");
        let parsed = Money::from_str(&s).ok();
        prop_assert_eq!(parsed, Some(m));
    }

    /// Property: Sum of iterator equals sequential addition.
    #[test]
    fn money_sum_equals_fold(values in prop::collection::vec(small_money_strategy(), 0..20)) {
        let sum_result: Money = values.iter().copied().sum();
        let fold_result = values.iter().copied().fold(Money::ZERO, |a, b| a + b);
        prop_assert_eq!(sum_result, fold_result);
    }

    /// Property: Money ordering is consistent.
    #[test]
    fn money_ordering_consistent(a in money_strategy(), b in money_strategy()) {
        // If a <= b and b <= a, then a == b
        if a <= b && b <= a {
            prop_assert_eq!(a, b);
        }
        // Ordering matches decimal ordering
        prop_assert_eq!(a.cmp(&b), a.as_decimal().cmp(&b.as_decimal()));
    }
}

// ============================================================================
// Bytes Property Tests
// ============================================================================

proptest! {
    /// Property: Bytes addition is commutative.
    #[test]
    fn bytes_addition_commutative(a in small_bytes_strategy(), b in small_bytes_strategy()) {
        prop_assert_eq!(a + b, b + a);
    }

    /// Property: Adding zero bytes doesn't change value.
    #[test]
    fn bytes_add_zero_identity(a in bytes_strategy()) {
        prop_assert_eq!(a + Bytes::ZERO, a);
        prop_assert_eq!(Bytes::ZERO + a, a);
    }

    /// Property: Bytes::ZERO.is_zero() is true.
    #[test]
    fn bytes_zero_is_zero(_x in 0..1i32) {
        prop_assert!(Bytes::ZERO.is_zero());
    }

    /// Property: Non-zero bytes is not zero.
    #[test]
    fn bytes_nonzero_is_not_zero(b in nonzero_bytes_strategy()) {
        prop_assert!(!b.is_zero());
    }

    /// Property: from_kb creates correct byte count.
    #[test]
    fn bytes_from_kb_correct(kb in 0u64..1_000_000u64) {
        let b = Bytes::from_kb(kb);
        prop_assert_eq!(b.as_bytes(), kb * 1024);
    }

    /// Property: from_mb creates correct byte count.
    #[test]
    fn bytes_from_mb_correct(mb in 0u64..1_000u64) {
        let b = Bytes::from_mb(mb);
        prop_assert_eq!(b.as_bytes(), mb * 1024 * 1024);
    }

    /// Property: from_gb creates correct byte count.
    #[test]
    fn bytes_from_gb_correct(gb in 0u64..1000u64) {
        let b = Bytes::from_gb(gb);
        prop_assert_eq!(b.as_bytes(), gb * 1024 * 1024 * 1024);
    }

    /// Property: KB bytes as_gb_decimal returns correct value.
    #[test]
    fn bytes_as_gb_decimal_correct(gb in 0u64..1000u64) {
        let b = Bytes::from_gb(gb);
        let gb_decimal = b.as_gb_decimal();
        prop_assert_eq!(gb_decimal, Decimal::from(gb));
    }

    /// Property: Bytes ordering is consistent with underlying byte count.
    #[test]
    fn bytes_ordering_consistent(a in bytes_strategy(), b in bytes_strategy()) {
        prop_assert_eq!(a.cmp(&b), a.as_bytes().cmp(&b.as_bytes()));
    }

    /// Property: max(a, b) is always >= both a and b.
    #[test]
    fn bytes_max_is_greater_or_equal(a in bytes_strategy(), b in bytes_strategy()) {
        let max = a.max(b);
        prop_assert!(max >= a);
        prop_assert!(max >= b);
    }
}

// ============================================================================
// TieredPrice Property Tests
// ============================================================================

proptest! {
    /// Property: Flat price cost is price * quantity.
    #[test]
    fn flat_price_cost_is_multiplicative(
        price_cents in 1u32..1000u32,
        gb in 0u64..10000u64
    ) {
        let price_str = format!("0.{price_cents:03}");
        let price = Money::from_str(&price_str).unwrap_or(Money::ZERO);
        let tiered = TieredPrice::flat(price);
        let cost = tiered.calculate_cost(Decimal::from(gb));
        let expected = price * Decimal::from(gb);
        prop_assert_eq!(cost, expected);
    }

    /// Property: Zero GB always costs zero.
    #[test]
    fn zero_gb_costs_zero(price in flat_price_strategy()) {
        let cost = price.calculate_cost(Decimal::ZERO);
        prop_assert!(cost.is_zero());
    }

    /// Property: base_price returns first tier price.
    #[test]
    fn base_price_returns_first_tier(price in flat_price_strategy()) {
        // For flat price, base_price equals the flat price
        match &price {
            TieredPrice::Flat(p) => prop_assert_eq!(price.base_price(), *p),
            TieredPrice::Tiered(_) => unreachable!(),
        }
    }

    /// Property: Cost increases monotonically with quantity (for flat pricing).
    #[test]
    fn flat_price_cost_monotonic(
        price in flat_price_strategy(),
        gb1 in 0u64..10000u64,
        gb2 in 0u64..10000u64
    ) {
        let cost1 = price.calculate_cost(Decimal::from(gb1));
        let cost2 = price.calculate_cost(Decimal::from(gb2));

        if gb1 <= gb2 {
            prop_assert!(cost1 <= cost2);
        } else {
            prop_assert!(cost1 >= cost2);
        }
    }

    /// Property: Tiered pricing is never more expensive than using highest price for all.
    #[test]
    fn tiered_never_more_than_max_price(
        tiered in tiered_price_strategy(),
        gb in gb_quantity_strategy()
    ) {
        let actual_cost = tiered.calculate_cost(gb);
        let max_price = tiered.base_price(); // First tier has highest price

        // Cost should never exceed max_price * gb
        let max_possible = max_price * gb;
        prop_assert!(actual_cost <= max_possible);
    }

    /// Property: TieredPrice default is flat zero.
    #[test]
    fn tiered_price_default_is_zero(_x in 0..1i32) {
        let default = TieredPrice::default();
        prop_assert_eq!(default.base_price(), Money::ZERO);
        prop_assert!(default.calculate_cost(Decimal::from(100)).is_zero());
    }
}

// ============================================================================
// Cross-Type Property Tests
// ============================================================================

proptest! {
    /// Property: Bytes to GB conversion preserves relative ordering.
    #[test]
    fn bytes_to_gb_preserves_order(a in gb_bytes_strategy(), b in gb_bytes_strategy()) {
        if a <= b {
            prop_assert!(a.as_gb_decimal() <= b.as_gb_decimal());
        } else {
            prop_assert!(a.as_gb_decimal() > b.as_gb_decimal());
        }
    }

    /// Property: Storage cost calculation is non-negative.
    #[test]
    fn storage_cost_non_negative(
        price in flat_price_strategy(),
        bytes in gb_bytes_strategy()
    ) {
        let gb = bytes.as_gb_decimal();
        let cost = price.calculate_cost(gb);
        prop_assert!(cost >= Money::ZERO);
    }
}

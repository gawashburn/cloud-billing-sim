//! Proptest strategies for Money type.

#![allow(dead_code)] // Strategies may be used by future tests

use cloud_billing_sim::types::Money;
use proptest::prelude::*;
use rust_decimal::Decimal;

/// Strategy for generating valid Money values.
/// Uses reasonable ranges for cloud billing (0 to 1 billion dollars).
pub fn money_strategy() -> impl Strategy<Value = Money> {
    // Generate cents from 0 to 100 billion cents ($1 billion)
    (0u64..100_000_000_000u64)
        .prop_map(|cents| Money::from_dollars(cents / 100, (cents % 100) as u32))
}

/// Strategy for small money values (for precise testing).
pub fn small_money_strategy() -> impl Strategy<Value = Money> {
    (0u64..10000u64, 0u32..100u32)
        .prop_map(|(dollars, cents)| Money::from_dollars(dollars, cents))
}

/// Strategy for generating non-zero Money values.
pub fn nonzero_money_strategy() -> impl Strategy<Value = Money> {
    (1u64..100_000_000_000u64)
        .prop_map(|cents| Money::from_dollars(cents / 100, (cents % 100) as u32))
}

/// Strategy for generating valid decimal multipliers.
pub fn decimal_multiplier_strategy() -> impl Strategy<Value = Decimal> {
    (0i64..1_000_000i64).prop_map(Decimal::from)
}

/// Strategy for quantity multipliers (u64).
pub fn quantity_strategy() -> impl Strategy<Value = u64> {
    0u64..1_000_000u64
}

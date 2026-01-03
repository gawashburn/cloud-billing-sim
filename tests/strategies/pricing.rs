//! Proptest strategies for pricing types.

#![allow(dead_code)] // Strategies may be used by future tests

use cloud_billing_sim::pricing::{PriceTier, TieredPrice};
use cloud_billing_sim::types::Money;
use proptest::prelude::*;
use std::str::FromStr;

/// Strategy for generating flat TieredPrice values.
pub fn flat_price_strategy() -> impl Strategy<Value = TieredPrice> {
    // Price per GB from $0.001 to $1.00
    (1u32..1000u32).prop_map(|millicents| {
        let price_str = format!("0.{millicents:03}");
        let money = Money::from_str(&price_str).unwrap_or(Money::ZERO);
        TieredPrice::flat(money)
    })
}

/// Strategy for generating price tiers.
pub fn price_tier_strategy() -> impl Strategy<Value = PriceTier> {
    (
        prop::option::of(1u64..1_000_000u64), // up_to_gb
        1u32..1000u32,                         // price in millicents
    )
        .prop_map(|(up_to_gb, millicents)| {
            let price_str = format!("0.{millicents:03}");
            PriceTier {
                up_to_gb,
                price: Money::from_str(&price_str).unwrap_or(Money::ZERO),
            }
        })
}

/// Strategy for generating tiered pricing with 2-4 tiers.
/// Ensures tiers are properly ordered (increasing thresholds, decreasing prices).
pub fn tiered_price_strategy() -> impl Strategy<Value = TieredPrice> {
    (2usize..=4usize).prop_flat_map(|num_tiers| {
        // Generate sorted thresholds and decreasing prices
        prop::collection::vec(1u64..100_000u64, num_tiers)
            .prop_flat_map(move |mut thresholds| {
                thresholds.sort();
                // Make last tier unlimited
                let thresholds: Vec<Option<u64>> = thresholds
                    .into_iter()
                    .take(num_tiers - 1)
                    .map(Some)
                    .chain(std::iter::once(None))
                    .collect();

                // Generate decreasing prices
                prop::collection::vec(1u32..1000u32, num_tiers).prop_map(move |mut prices| {
                    prices.sort();
                    prices.reverse(); // Higher price for lower tiers

                    let tiers: Vec<PriceTier> = thresholds
                        .iter()
                        .zip(prices.iter())
                        .map(|(&up_to_gb, &millicents)| {
                            let price_str = format!("0.{millicents:03}");
                            PriceTier {
                                up_to_gb,
                                price: Money::from_str(&price_str).unwrap_or(Money::ZERO),
                            }
                        })
                        .collect();

                    TieredPrice::tiered(tiers)
                })
            })
    })
}

/// Strategy for GB quantities for cost calculation.
pub fn gb_quantity_strategy() -> impl Strategy<Value = rust_decimal::Decimal> {
    (0u64..1_000_000u64).prop_map(rust_decimal::Decimal::from)
}

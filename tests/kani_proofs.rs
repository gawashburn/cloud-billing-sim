//! Kani formal verification proofs for cloud billing simulator.
//!
//! Run with: `cargo kani`
//!
//! These proofs verify algebraic properties and invariants of the core types.

#![cfg(kani)]

use cloud_billing_sim::pricing::{PriceTier, TieredPrice};
use cloud_billing_sim::types::{Bytes, Money};
use rust_decimal::Decimal;

// ============================================================================
// Money Arbitrary Implementation
// ============================================================================

impl kani::Arbitrary for Money {
    fn any() -> Self {
        // Generate bounded money values (0 to $1,000,000.00)
        // Using cents to avoid decimal complexity in verification
        let cents: u64 = kani::any();
        kani::assume(cents <= 100_000_000); // Max $1M
        Self::from_dollars(cents / 100, (cents % 100) as u32)
    }
}

// ============================================================================
// Bytes Arbitrary Implementation
// ============================================================================

impl kani::Arbitrary for Bytes {
    fn any() -> Self {
        // Generate bounded byte values (0 to 1 PB)
        // Using smaller range for tractable verification
        let bytes: u64 = kani::any();
        kani::assume(bytes <= 1_000_000_000_000_000); // Max 1 PB
        Self::new(bytes)
    }
}

// ============================================================================
// Money Proofs
// ============================================================================

/// Verifies that Money addition is commutative: a + b == b + a
#[kani::proof]
#[kani::unwind(2)]
fn verify_money_addition_commutative() {
    let a: Money = kani::any();
    let b: Money = kani::any();

    assert!(a + b == b + a, "Addition must be commutative");
}

/// Verifies that adding zero doesn't change the value: a + ZERO == a
#[kani::proof]
#[kani::unwind(2)]
fn verify_money_zero_identity() {
    let a: Money = kani::any();

    assert!(a + Money::ZERO == a, "Zero must be additive identity");
    assert!(
        Money::ZERO + a == a,
        "Zero must be additive identity (left)"
    );
}

/// Verifies that Money::ZERO.is_zero() returns true
#[kani::proof]
fn verify_money_zero_is_zero() {
    assert!(Money::ZERO.is_zero(), "ZERO constant must be zero");
}

/// Verifies that multiplying by zero yields zero
#[kani::proof]
#[kani::unwind(2)]
fn verify_money_multiply_by_zero() {
    let a: Money = kani::any();

    assert!((a * 0u64).is_zero(), "Multiplying by zero must yield zero");
}

/// Verifies that multiplying by one doesn't change the value
#[kani::proof]
#[kani::unwind(2)]
fn verify_money_multiply_by_one() {
    let a: Money = kani::any();

    assert!(a * 1u64 == a, "Multiplying by one must be identity");
}

/// Verifies that from_dollars creates consistent values
#[kani::proof]
fn verify_money_from_dollars_consistency() {
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
fn verify_money_nonzero_is_not_zero() {
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
fn verify_money_ordering_consistent() {
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

// ============================================================================
// Bytes Proofs
// ============================================================================

/// Verifies that Bytes addition is commutative: a + b == b + a
#[kani::proof]
#[kani::unwind(2)]
fn verify_bytes_addition_commutative() {
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
fn verify_bytes_zero_identity() {
    let a: Bytes = kani::any();

    assert!(a + Bytes::ZERO == a, "Zero must be additive identity");
    assert!(
        Bytes::ZERO + a == a,
        "Zero must be additive identity (left)"
    );
}

/// Verifies that Bytes::ZERO.is_zero() returns true
#[kani::proof]
fn verify_bytes_zero_is_zero() {
    assert!(Bytes::ZERO.is_zero(), "ZERO constant must be zero");
}

/// Verifies non-zero bytes is not zero
#[kani::proof]
fn verify_bytes_nonzero_is_not_zero() {
    let bytes: u64 = kani::any();
    kani::assume(bytes > 0);
    kani::assume(bytes <= 1_000_000_000_000);

    let b = Bytes::new(bytes);
    assert!(!b.is_zero(), "Non-zero bytes must not be zero");
}

/// Verifies saturating_sub never underflows
#[kani::proof]
#[kani::unwind(2)]
fn verify_bytes_saturating_sub_no_underflow() {
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
fn verify_bytes_max_returns_greater() {
    let a: Bytes = kani::any();
    let b: Bytes = kani::any();

    let max = a.max(b);

    assert!(max >= a, "max must be >= a");
    assert!(max >= b, "max must be >= b");
    assert!(max == a || max == b, "max must equal a or b");
}

/// Verifies from_kb creates correct byte count
#[kani::proof]
fn verify_bytes_from_kb_correct() {
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
fn verify_bytes_from_mb_correct() {
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
fn verify_bytes_from_gb_correct() {
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
fn verify_bytes_ordering_consistent() {
    let a: Bytes = kani::any();
    let b: Bytes = kani::any();

    // Ordering matches underlying u64 ordering
    assert!(
        a.cmp(&b) == a.as_bytes().cmp(&b.as_bytes()),
        "Bytes ordering must match u64 ordering"
    );
}

// ============================================================================
// TieredPrice Proofs
// ============================================================================

/// Verifies that zero GB always costs zero for flat pricing
#[kani::proof]
fn verify_tiered_zero_gb_costs_zero_flat() {
    let price: Money = kani::any();
    let tiered = TieredPrice::flat(price);

    let cost = tiered.calculate_cost(Decimal::ZERO);
    assert!(cost.is_zero(), "Zero GB must cost zero");
}

/// Verifies that flat price cost equals price * quantity
#[kani::proof]
fn verify_tiered_flat_price_multiplicative() {
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

    assert!(
        cost == expected,
        "Flat price cost must equal price * quantity"
    );
}

/// Verifies that cost is non-negative for flat pricing
#[kani::proof]
fn verify_tiered_cost_non_negative() {
    let price: Money = kani::any();
    let gb: u64 = kani::any();
    kani::assume(gb <= 10_000);

    let tiered = TieredPrice::flat(price);
    let cost = tiered.calculate_cost(Decimal::from(gb));

    assert!(cost >= Money::ZERO, "Cost must be non-negative");
}

/// Verifies base_price returns the flat price for flat pricing
#[kani::proof]
fn verify_tiered_base_price_flat() {
    let price: Money = kani::any();
    let tiered = TieredPrice::flat(price);

    assert!(
        tiered.base_price() == price,
        "base_price must return flat price"
    );
}

/// Verifies default TieredPrice is flat zero
#[kani::proof]
fn verify_tiered_default_is_flat_zero() {
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
fn verify_tiered_flat_cost_monotonic() {
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

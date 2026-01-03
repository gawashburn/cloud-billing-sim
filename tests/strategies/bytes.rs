//! Proptest strategies for Bytes type.

#![allow(dead_code)] // Strategies may be used by future tests

use cloud_billing_sim::types::Bytes;
use proptest::prelude::*;

/// Strategy for generating valid Bytes values.
/// Range: 0 to 1 PB (reasonable for cloud storage).
pub fn bytes_strategy() -> impl Strategy<Value = Bytes> {
    (0u64..1_000_000_000_000_000u64).prop_map(Bytes::new)
}

/// Strategy for small byte values (for precise testing).
pub fn small_bytes_strategy() -> impl Strategy<Value = Bytes> {
    (0u64..1_000_000_000u64).prop_map(Bytes::new) // Up to 1 GB
}

/// Strategy for KB-sized values.
pub fn kb_bytes_strategy() -> impl Strategy<Value = Bytes> {
    (0u64..1_000_000u64).prop_map(Bytes::from_kb)
}

/// Strategy for MB-sized values.
pub fn mb_bytes_strategy() -> impl Strategy<Value = Bytes> {
    (0u64..1_000_000u64).prop_map(Bytes::from_mb)
}

/// Strategy for GB-sized values.
pub fn gb_bytes_strategy() -> impl Strategy<Value = Bytes> {
    (0u64..10_000u64).prop_map(Bytes::from_gb)
}

/// Strategy for non-zero bytes.
pub fn nonzero_bytes_strategy() -> impl Strategy<Value = Bytes> {
    (1u64..1_000_000_000_000_000u64).prop_map(Bytes::new)
}

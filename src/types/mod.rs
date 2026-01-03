//! Core types for the cloud billing simulator.
//!
//! This module provides fundamental types used throughout the simulator:
//! - [`Money`] - Precise decimal representation of monetary values
//! - [`Bytes`] - Storage size representation with unit conversions
//! - [`StorageClass`] - Identifier for storage tiers

mod bytes;
mod money;
mod storage_class;

pub use bytes::Bytes;
pub use money::Money;
pub use storage_class::StorageClass;

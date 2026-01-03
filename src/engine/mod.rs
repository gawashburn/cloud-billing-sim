//! Cost calculation engine.
//!
//! The engine processes a sequence of operations against pricing rules
//! to compute total costs. It tracks object state (storage class, size,
//! creation time) to properly calculate storage duration costs.

mod error;
mod report;
mod simulator;
mod state;

pub use error::EngineError;
pub use report::{CostBreakdown, CostReport, ObjectCosts};
pub use simulator::Simulator;
pub use state::{ObjectState, StorageState};

//! Cloud Billing Simulator
//!
//! A tool for computing cloud object storage costs from API operations.
//!
//! This crate provides:
//! - A domain-specific language for describing storage pricing rules (TOML-based)
//! - A parser for operation logs (JSON format)
//! - A simulation engine that calculates costs based on operations and rules
//!
//! # Example
//!
//! ```no_run
//! use cloud_billing_sim::{pricing, operations, engine};
//!
//! // Load pricing rules
//! let rules = pricing::load_rules("pricing/aws-s3-us-east-1.toml")?;
//!
//! // Load operations
//! let ops = operations::load_operations("workload.json")?;
//!
//! // Run simulation
//! let mut sim = engine::Simulator::new(rules);
//! let report = sim.simulate(&ops)?;
//!
//! println!("Total cost: {}", report.total_cost);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Pricing Rules DSL
//!
//! Pricing rules are defined in TOML format:
//!
//! ```toml
//! [provider]
//! name = "aws-s3"
//! region = "us-east-1"
//!
//! [storage_classes.STANDARD]
//! storage_price_per_gb_month = "0.023"
//!
//! [storage_classes.STANDARD_IA]
//! storage_price_per_gb_month = "0.0125"
//! min_billable_size_bytes = 131072  # 128 KB
//! min_storage_duration_days = 30
//! retrieval_price_per_gb = "0.01"
//!
//! [operations.DEFAULT]
//! put_per_1000 = "0.005"
//! get_per_1000 = "0.0004"
//! ```
//!
//! # Operations Format
//!
//! Operations are provided as JSON:
//!
//! ```json
//! {
//!   "operations": [
//!     {
//!       "timestamp": "2024-01-15T10:30:00Z",
//!       "operation": "put_object",
//!       "bucket": "my-bucket",
//!       "key": "path/to/file.txt",
//!       "size_bytes": 1048576,
//!       "storage_class": "STANDARD"
//!     }
//!   ]
//! }
//! ```

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod engine;
pub mod operations;
pub mod pricing;
pub mod types;

#[cfg(any(feature = "s3-validation", feature = "b2-validation"))]
pub mod validation;

// Re-export commonly used types
pub use engine::{CostReport, Simulator};
pub use operations::OperationLog;
pub use pricing::PricingRules;
pub use types::{Bytes, Money, StorageClass};

//! Validation module for testing simulations against real cloud providers.
//!
//! This module provides traits and implementations for validating
//! billing simulator results against actual cloud storage services.
//!
//! # Features
//!
//! - `s3-validation`: Enables AWS S3 validation support
//! - `b2-validation`: Enables Backblaze B2 validation support
//! - `r2-validation`: Enables Cloudflare R2 validation support
//! - `azure-validation`: Enables Azure Blob Storage validation support
//!
//! # Architecture
//!
//! The validation system works by:
//! 1. Executing a sequence of operations against the real cloud provider
//! 2. Running the same operations through the simulator
//! 3. Comparing the simulated costs with actual billing data
//!
//! # Example
//!
//! ```no_run,ignore
//! use cloud_billing_sim::validation::{ValidationProvider, ValidationResult};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create provider-specific validator
//! let validator = S3Validator::new("my-test-bucket", "us-east-1").await?;
//!
//! // Run validation workload
//! let result = validator.validate_workload(&operations, &pricing_rules).await?;
//!
//! // Check accuracy
//! assert!(result.accuracy_percentage() > 95.0);
//! # Ok(())
//! # }
//! ```

mod error;
mod traits;

#[cfg(feature = "s3-validation")]
pub mod s3;

#[cfg(feature = "b2-validation")]
pub mod b2;

#[cfg(feature = "r2-validation")]
pub mod r2;

#[cfg(feature = "azure-validation")]
pub mod azure;

pub use error::ValidationError;
pub use traits::{
    ActualCosts, CostComparison, ExecutedWorkload, ValidationProvider, ValidationResult,
    ValidationWorkload,
};

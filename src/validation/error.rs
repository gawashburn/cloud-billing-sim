//! Validation error types.

use thiserror::Error;

/// Errors that can occur during validation.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// AWS SDK configuration error.
    #[cfg(any(feature = "s3-validation", feature = "r2-validation"))]
    #[error("AWS configuration error: {0}")]
    AwsConfig(String),

    /// S3-compatible operation failed (used by S3 and R2).
    #[cfg(any(feature = "s3-validation", feature = "r2-validation"))]
    #[error("S3-compatible operation failed: {0}")]
    S3Operation(String),

    /// B2 authentication error.
    #[cfg(feature = "b2-validation")]
    #[error("B2 authentication failed: {0}")]
    B2Auth(String),

    /// B2 operation failed.
    #[cfg(feature = "b2-validation")]
    #[error("B2 operation failed: {0}")]
    B2Operation(String),

    /// HTTP request error.
    #[cfg(feature = "b2-validation")]
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Billing data not available.
    #[error("billing data not available: {0}")]
    BillingNotAvailable(String),

    /// Invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Operation timeout.
    #[error("operation timed out after {0} seconds")]
    Timeout(u64),

    /// Bucket not found or inaccessible.
    #[error("bucket not accessible: {0}")]
    BucketNotAccessible(String),

    /// Simulation error.
    #[error("simulation error: {0}")]
    Simulation(String),

    /// Cost comparison failed.
    #[error("cost comparison failed: {0}")]
    CostMismatch(String),
}

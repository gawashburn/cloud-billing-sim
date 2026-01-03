//! Pricing module errors.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when loading or applying pricing rules.
#[derive(Debug, Error)]
pub enum PricingError {
    /// Failed to read pricing rules file.
    #[error("failed to read pricing file {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),

    /// Failed to parse TOML pricing rules.
    #[error("failed to parse pricing rules: {0}")]
    Parse(#[from] toml::de::Error),

    /// Unknown storage class referenced.
    #[error("unknown storage class: {0}")]
    UnknownStorageClass(String),

    /// Unknown retrieval tier.
    #[error("unknown retrieval tier '{tier}' for storage class '{storage_class}'")]
    UnknownRetrievalTier {
        /// The storage class name.
        storage_class: String,
        /// The retrieval tier name.
        tier: String,
    },

    /// Missing required pricing rule.
    #[error("missing pricing rule: {0}")]
    MissingRule(String),
}

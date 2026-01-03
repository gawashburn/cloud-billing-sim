//! Operation module errors.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when loading or processing operations.
#[derive(Debug, Error)]
pub enum OperationError {
    /// Failed to read operations file.
    #[error("failed to read operations file {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),

    /// Failed to parse JSON operations.
    #[error("failed to parse operations: {0}")]
    Parse(#[from] serde_json::Error),

    /// Invalid operation data.
    #[error("invalid operation: {0}")]
    Invalid(String),

    /// Operation references unknown object.
    #[error("operation references unknown object: {bucket}/{key}")]
    UnknownObject {
        /// The bucket name.
        bucket: String,
        /// The object key.
        key: String,
    },
}

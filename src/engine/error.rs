//! Engine errors.

use thiserror::Error;

/// Errors that can occur during cost simulation.
#[derive(Debug, Error)]
pub enum EngineError {
    /// Pricing rules error.
    #[error("pricing error: {0}")]
    Pricing(#[from] crate::pricing::PricingError),

    /// Operations error.
    #[error("operations error: {0}")]
    Operations(#[from] crate::operations::OperationError),

    /// Operation on unknown object.
    #[error("operation on unknown object: {bucket}/{key}")]
    UnknownObject {
        /// The bucket name.
        bucket: String,
        /// The object key.
        key: String,
    },

    /// Unknown storage class.
    #[error("unknown storage class: {0}")]
    UnknownStorageClass(String),

    /// Invalid operation sequence.
    #[error("invalid operation sequence: {0}")]
    InvalidSequence(String),
}

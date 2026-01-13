//! Cloud storage operation definitions and parsing.
//!
//! This module defines the input format for storage operations to be costed.
//! Operations are provided as JSON and include timestamps for duration calculations.

mod error;
mod types;

pub use error::OperationError;
pub use types::{Operation, OperationKind, OperationLog, RetrievalSpeed};

use std::path::Path;

/// Loads operations from a JSON file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
///
/// # Examples
///
/// ```no_run
/// use cloud_billing_sim::operations::load_operations;
///
/// let ops = load_operations("operations.json")?;
/// for op in ops.operations {
///     println!("{}: {:?}", op.timestamp, op.kind);
/// }
/// # Ok::<(), cloud_billing_sim::operations::OperationError>(())
/// ```
pub fn load_operations(path: impl AsRef<Path>) -> Result<OperationLog, OperationError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| OperationError::Io(path.as_ref().to_path_buf(), e))?;
    parse_operations(&content)
}

/// Parses operations from a JSON string.
///
/// # Errors
///
/// Returns an error if the JSON is invalid.
pub fn parse_operations(json_content: &str) -> Result<OperationLog, OperationError> {
    serde_json::from_str(json_content).map_err(OperationError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_operations_valid_json() {
        let json = r#"{
            "operations": [
                {
                    "timestamp": "2024-01-01T00:00:00Z",
                    "operation": "put_object",
                    "bucket": "test-bucket",
                    "key": "file.txt",
                    "size_bytes": 1024
                }
            ]
        }"#;

        let log = parse_operations(json).expect("should parse valid JSON");
        assert_eq!(log.operations.len(), 1);
    }

    #[test]
    fn parse_operations_invalid_json() {
        let json = "not valid json";
        let result = parse_operations(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_operations_empty_operations() {
        let json = r#"{"operations": []}"#;
        let log = parse_operations(json).expect("should parse empty operations");
        assert!(log.operations.is_empty());
    }
}

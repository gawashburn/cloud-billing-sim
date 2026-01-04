//! Operation type definitions.

use crate::types::StorageClass;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A log of operations to be costed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLog {
    /// List of operations in chronological order.
    pub operations: Vec<Operation>,

    /// Optional metadata about the log.
    #[serde(default)]
    pub metadata: Option<LogMetadata>,
}

impl OperationLog {
    /// Creates a new empty operation log.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            operations: Vec::new(),
            metadata: None,
        }
    }

    /// Returns operations sorted by timestamp.
    #[must_use]
    pub fn sorted(&self) -> Vec<&Operation> {
        let mut ops: Vec<_> = self.operations.iter().collect();
        ops.sort_by_key(|op| op.timestamp);
        ops
    }

    /// Returns the time range of the operations.
    #[must_use]
    pub fn time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let sorted = self.sorted();
        let first = sorted.first()?.timestamp;
        let last = sorted.last()?.timestamp;
        Some((first, last))
    }
}

impl Default for OperationLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata about the operation log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMetadata {
    /// Description of the workload.
    #[serde(default)]
    pub description: Option<String>,

    /// Source system that generated the log.
    #[serde(default)]
    pub source: Option<String>,
}

/// A single storage operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// When the operation occurred.
    pub timestamp: DateTime<Utc>,

    /// The type of operation.
    #[serde(flatten)]
    pub kind: OperationKind,

    /// Bucket name.
    pub bucket: String,

    /// Object key (path within bucket).
    #[serde(default)]
    pub key: Option<String>,
}

impl Operation {
    /// Returns a display name for the object.
    #[must_use]
    pub fn object_path(&self) -> String {
        self.key.as_ref().map_or_else(
            || self.bucket.clone(),
            |key| format!("{}/{}", self.bucket, key),
        )
    }
}

/// The kind of storage operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum OperationKind {
    /// Upload/create an object.
    PutObject {
        /// Size of the object in bytes.
        #[serde(default)]
        size_bytes: u64,

        /// Storage class for the object.
        #[serde(default = "default_storage_class")]
        storage_class: StorageClass,
    },

    /// Copy an object (within same bucket or across buckets).
    CopyObject {
        /// Source key.
        source_key: String,

        /// Source bucket (if different).
        #[serde(default)]
        source_bucket: Option<String>,

        /// Destination storage class.
        #[serde(default)]
        storage_class: Option<StorageClass>,
    },

    /// Download/retrieve an object.
    GetObject {
        /// Bytes transferred (may be partial).
        #[serde(default)]
        bytes_transferred: Option<u64>,

        /// Retrieval speed tier (for archive classes).
        #[serde(default)]
        retrieval_tier: Option<RetrievalSpeed>,
    },

    /// Delete an object.
    DeleteObject,

    /// List objects in a bucket.
    ListObjects {
        /// Number of objects returned.
        #[serde(default)]
        objects_returned: Option<u32>,
    },

    /// Head request (metadata only).
    HeadObject,

    /// Initiate a multipart upload.
    CreateMultipartUpload {
        /// Storage class for the upload.
        #[serde(default = "default_storage_class")]
        storage_class: StorageClass,
    },

    /// Upload a part of a multipart upload.
    UploadPart {
        /// Size of this part in bytes.
        size_bytes: u64,

        /// Upload ID.
        upload_id: String,

        /// Part number.
        part_number: u32,
    },

    /// Complete a multipart upload.
    CompleteMultipartUpload {
        /// Upload ID.
        upload_id: String,

        /// Total size of the completed object.
        #[serde(default)]
        total_size_bytes: Option<u64>,
    },

    /// Abort a multipart upload.
    AbortMultipartUpload {
        /// Upload ID.
        upload_id: String,
    },

    /// Restore an archived object.
    RestoreObject {
        /// Number of days to keep restored copy.
        days: u32,

        /// Retrieval speed tier.
        #[serde(default)]
        tier: Option<RetrievalSpeed>,
    },

    /// Lifecycle transition (typically automated).
    LifecycleTransition {
        /// New storage class.
        new_storage_class: StorageClass,
    },

    /// Select content from an object.
    SelectObjectContent {
        /// Bytes scanned.
        #[serde(default)]
        bytes_scanned: Option<u64>,

        /// Bytes returned.
        #[serde(default)]
        bytes_returned: Option<u64>,
    },

    /// Wait/advance time without performing an operation.
    ///
    /// This is used to calculate storage costs for a specific duration.
    /// The simulator will bill all storage up to this timestamp.
    Wait {
        /// Optional description of why the wait was added.
        #[serde(default)]
        reason: Option<String>,
    },
}

fn default_storage_class() -> StorageClass {
    StorageClass::new("STANDARD")
}

/// Retrieval speed tiers for archive storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetrievalSpeed {
    /// Fastest retrieval (1-5 minutes).
    Expedited,
    /// Standard retrieval (3-5 hours).
    Standard,
    /// Slowest/cheapest retrieval (5-12 hours).
    Bulk,
}

impl RetrievalSpeed {
    /// Returns the tier name as a string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expedited => "expedited",
            Self::Standard => "standard",
            Self::Bulk => "bulk",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_put_object() -> Result<(), serde_json::Error> {
        let json = r#"{
            "timestamp": "2024-01-15T10:30:00Z",
            "operation": "put_object",
            "bucket": "my-bucket",
            "key": "path/to/file.txt",
            "size_bytes": 1048576,
            "storage_class": "STANDARD"
        }"#;

        let op: Operation = serde_json::from_str(json)?;
        assert_eq!(op.bucket, "my-bucket");
        assert!(matches!(op.kind, OperationKind::PutObject { .. }));
        Ok(())
    }

    #[test]
    fn parse_get_object() -> Result<(), serde_json::Error> {
        let json = r#"{
            "timestamp": "2024-01-15T11:00:00Z",
            "operation": "get_object",
            "bucket": "my-bucket",
            "key": "path/to/file.txt",
            "bytes_transferred": 1048576
        }"#;

        let op: Operation = serde_json::from_str(json)?;
        assert!(matches!(
            op.kind,
            OperationKind::GetObject {
                bytes_transferred: Some(1_048_576),
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn parse_operation_log() -> Result<(), serde_json::Error> {
        let json = r#"{
            "operations": [
                {
                    "timestamp": "2024-01-15T10:30:00Z",
                    "operation": "put_object",
                    "bucket": "my-bucket",
                    "key": "file1.txt",
                    "size_bytes": 1000
                },
                {
                    "timestamp": "2024-01-15T10:31:00Z",
                    "operation": "get_object",
                    "bucket": "my-bucket",
                    "key": "file1.txt"
                }
            ]
        }"#;

        let log: OperationLog = serde_json::from_str(json)?;
        assert_eq!(log.operations.len(), 2);
        Ok(())
    }

    #[test]
    fn parse_wait_operation() -> Result<(), serde_json::Error> {
        let json = r#"{
            "timestamp": "2024-02-15T00:00:00Z",
            "operation": "wait",
            "bucket": "_",
            "reason": "Calculate 30 days of storage costs"
        }"#;

        let op: Operation = serde_json::from_str(json)?;
        assert_eq!(op.bucket, "_");
        assert!(matches!(
            op.kind,
            OperationKind::Wait {
                reason: Some(ref r)
            } if r == "Calculate 30 days of storage costs"
        ));
        Ok(())
    }

    #[test]
    fn parse_wait_operation_no_reason() -> Result<(), serde_json::Error> {
        let json = r#"{
            "timestamp": "2024-02-15T00:00:00Z",
            "operation": "wait",
            "bucket": "_"
        }"#;

        let op: Operation = serde_json::from_str(json)?;
        assert!(matches!(op.kind, OperationKind::Wait { reason: None }));
        Ok(())
    }

    #[test]
    fn time_range_with_wait() {
        let json = r#"{
            "operations": [
                {
                    "timestamp": "2024-01-01T00:00:00Z",
                    "operation": "put_object",
                    "bucket": "my-bucket",
                    "key": "file.txt",
                    "size_bytes": 1000
                },
                {
                    "timestamp": "2024-07-01T00:00:00Z",
                    "operation": "wait",
                    "bucket": "_",
                    "reason": "6 months storage"
                }
            ]
        }"#;

        let log: OperationLog = serde_json::from_str(json).expect("parse failed");
        let (start, end) = log.time_range().expect("time_range failed");

        // Should span 6 months
        assert_eq!(start.format("%Y-%m-%d").to_string(), "2024-01-01");
        assert_eq!(end.format("%Y-%m-%d").to_string(), "2024-07-01");
    }
}

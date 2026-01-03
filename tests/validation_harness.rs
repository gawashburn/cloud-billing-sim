//! Integration test harness for cloud provider validation.
//!
//! This module provides utilities for running validation tests against
//! real cloud providers (S3, B2).
//!
//! # Running Tests
//!
//! These tests require actual cloud credentials and will incur real costs.
//! Run with:
//!
//! ```bash
//! # S3 validation tests
//! cargo test --features s3-validation validation_s3 -- --ignored
//!
//! # B2 validation tests
//! cargo test --features b2-validation validation_b2 -- --ignored
//! ```
//!
//! # Environment Variables
//!
//! For S3:
//! - AWS credentials via standard AWS SDK methods
//! - `TEST_S3_BUCKET`: S3 bucket name
//! - `TEST_S3_REGION`: AWS region (default: us-east-1)
//!
//! For B2:
//! - `B2_APPLICATION_KEY_ID`: B2 key ID
//! - `B2_APPLICATION_KEY`: B2 application key
//! - `TEST_B2_BUCKET`: B2 bucket name

#![allow(dead_code)]

use cloud_billing_sim::operations::{Operation, OperationKind, OperationLog};
use cloud_billing_sim::types::StorageClass;
use chrono::Utc;

/// Creates a simple test workload with basic operations.
pub fn simple_workload() -> OperationLog {
    let now = Utc::now();

    OperationLog {
        operations: vec![
            // Upload a small file
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("test-file-1.txt".to_string()),
                kind: OperationKind::PutObject {
                    size_bytes: 1024,
                    storage_class: StorageClass::new("STANDARD"),
                },
            },
            // Upload a larger file
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("test-file-2.bin".to_string()),
                kind: OperationKind::PutObject {
                    size_bytes: 1024 * 1024, // 1 MB
                    storage_class: StorageClass::new("STANDARD"),
                },
            },
            // List objects
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("test-".to_string()),
                kind: OperationKind::ListObjects {
                    objects_returned: Some(2),
                },
            },
            // Download the file
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("test-file-1.txt".to_string()),
                kind: OperationKind::GetObject {
                    bytes_transferred: Some(1024),
                    retrieval_tier: None,
                },
            },
            // Delete the files
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("test-file-1.txt".to_string()),
                kind: OperationKind::DeleteObject,
            },
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("test-file-2.bin".to_string()),
                kind: OperationKind::DeleteObject,
            },
        ],
        metadata: None,
    }
}

/// Creates a storage-heavy workload for testing storage costs.
pub fn storage_workload(num_files: usize, file_size_bytes: u64) -> OperationLog {
    let now = Utc::now();
    let mut operations = Vec::with_capacity(num_files);

    for i in 0..num_files {
        operations.push(Operation {
            timestamp: now,
            bucket: "test".to_string(),
            key: Some(format!("storage-test/file-{i:04}.bin")),
            kind: OperationKind::PutObject {
                size_bytes: file_size_bytes,
                storage_class: StorageClass::new("STANDARD"),
            },
        });
    }

    OperationLog {
        operations,
        metadata: None,
    }
}

/// Creates an operations-heavy workload for testing operation costs.
pub fn operations_workload(num_puts: usize, num_gets: usize, num_lists: usize) -> OperationLog {
    let now = Utc::now();
    let mut operations = Vec::new();

    // First create some files
    for i in 0..num_puts.min(100) {
        operations.push(Operation {
            timestamp: now,
            bucket: "test".to_string(),
            key: Some(format!("ops-test/file-{i:04}.txt")),
            kind: OperationKind::PutObject {
                size_bytes: 100,
                storage_class: StorageClass::new("STANDARD"),
            },
        });
    }

    // Then read them
    for i in 0..num_gets {
        let file_idx = i % num_puts.min(100);
        operations.push(Operation {
            timestamp: now,
            bucket: "test".to_string(),
            key: Some(format!("ops-test/file-{file_idx:04}.txt")),
            kind: OperationKind::GetObject {
                bytes_transferred: Some(100),
                retrieval_tier: None,
            },
        });
    }

    // List operations
    for _ in 0..num_lists {
        operations.push(Operation {
            timestamp: now,
            bucket: "test".to_string(),
            key: Some("ops-test/".to_string()),
            kind: OperationKind::ListObjects {
                objects_returned: Some(100),
            },
        });
    }

    OperationLog {
        operations,
        metadata: None,
    }
}

/// Creates a mixed workload with various operation types.
pub fn mixed_workload() -> OperationLog {
    let now = Utc::now();

    OperationLog {
        operations: vec![
            // Create files in different storage classes
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("mixed/standard.txt".to_string()),
                kind: OperationKind::PutObject {
                    size_bytes: 10 * 1024,
                    storage_class: StorageClass::new("STANDARD"),
                },
            },
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("mixed/ia.txt".to_string()),
                kind: OperationKind::PutObject {
                    size_bytes: 256 * 1024,
                    storage_class: StorageClass::new("STANDARD_IA"),
                },
            },
            // Head request
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("mixed/standard.txt".to_string()),
                kind: OperationKind::HeadObject,
            },
            // Get requests
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("mixed/standard.txt".to_string()),
                kind: OperationKind::GetObject {
                    bytes_transferred: Some(10 * 1024),
                    retrieval_tier: None,
                },
            },
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("mixed/ia.txt".to_string()),
                kind: OperationKind::GetObject {
                    bytes_transferred: Some(256 * 1024),
                    retrieval_tier: None,
                },
            },
            // List
            Operation {
                timestamp: now,
                bucket: "test".to_string(),
                key: Some("mixed/".to_string()),
                kind: OperationKind::ListObjects {
                    objects_returned: Some(2),
                },
            },
        ],
        metadata: None,
    }
}

/// Checks if S3 validation environment is configured.
pub fn s3_env_configured() -> bool {
    std::env::var("TEST_S3_BUCKET").is_ok()
}

/// Checks if B2 validation environment is configured.
pub fn b2_env_configured() -> bool {
    std::env::var("B2_APPLICATION_KEY_ID").is_ok()
        && std::env::var("B2_APPLICATION_KEY").is_ok()
        && std::env::var("TEST_B2_BUCKET").is_ok()
}

/// Gets the test S3 bucket name.
pub fn get_s3_bucket() -> Option<String> {
    std::env::var("TEST_S3_BUCKET").ok()
}

/// Gets the test S3 region.
pub fn get_s3_region() -> String {
    std::env::var("TEST_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string())
}

/// Gets the test B2 bucket name.
pub fn get_b2_bucket() -> Option<String> {
    std::env::var("TEST_B2_BUCKET").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_workload_has_operations() {
        let workload = simple_workload();
        assert!(!workload.operations.is_empty());
    }

    #[test]
    fn storage_workload_creates_files() {
        let workload = storage_workload(10, 1024);
        assert_eq!(workload.operations.len(), 10);

        for op in &workload.operations {
            assert!(matches!(op.kind, OperationKind::PutObject { .. }));
        }
    }

    #[test]
    fn operations_workload_has_variety() {
        let workload = operations_workload(10, 20, 5);

        let puts = workload
            .operations
            .iter()
            .filter(|op| matches!(op.kind, OperationKind::PutObject { .. }))
            .count();
        let gets = workload
            .operations
            .iter()
            .filter(|op| matches!(op.kind, OperationKind::GetObject { .. }))
            .count();
        let lists = workload
            .operations
            .iter()
            .filter(|op| matches!(op.kind, OperationKind::ListObjects { .. }))
            .count();

        assert_eq!(puts, 10);
        assert_eq!(gets, 20);
        assert_eq!(lists, 5);
    }

    #[test]
    fn mixed_workload_has_variety() {
        let workload = mixed_workload();

        let has_put = workload
            .operations
            .iter()
            .any(|op| matches!(op.kind, OperationKind::PutObject { .. }));
        let has_get = workload
            .operations
            .iter()
            .any(|op| matches!(op.kind, OperationKind::GetObject { .. }));
        let has_head = workload
            .operations
            .iter()
            .any(|op| matches!(op.kind, OperationKind::HeadObject));
        let has_list = workload
            .operations
            .iter()
            .any(|op| matches!(op.kind, OperationKind::ListObjects { .. }));

        assert!(has_put);
        assert!(has_get);
        assert!(has_head);
        assert!(has_list);
    }
}

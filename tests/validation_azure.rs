//! Azure Blob Storage validation integration tests.
//!
//! These tests validate the billing simulator against real Azure Blob Storage.
//!
//! # Prerequisites
//!
//! - Environment variables:
//!   - `AZURE_STORAGE_ACCOUNT`: Azure storage account name
//!   - `AZURE_STORAGE_ACCESS_KEY`: Storage account access key
//!   - `TEST_AZURE_CONTAINER`: Test container name
//!
//! # Running
//!
//! ```bash
//! cargo test --features azure-validation validation_azure -- --ignored
//! ```

#![cfg(feature = "azure-validation")]
#![allow(clippy::expect_used)]

mod validation_harness;

use cloud_billing_sim::pricing;
use cloud_billing_sim::validation::azure::AzureValidator;
use cloud_billing_sim::validation::{ValidationProvider, ValidationWorkload};
#[allow(unused_imports)]
use validation_harness::{
    azure_env_configured, get_azure_container, mixed_workload, operations_workload,
    simple_workload, storage_workload,
};

/// Test Azure validator initialization.
#[tokio::test]
#[ignore = "requires Azure credentials and test container"]
async fn test_azure_validator_init() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let container = get_azure_container().expect("TEST_AZURE_CONTAINER not set");
    let validator = AzureValidator::from_env(&container).await;

    assert!(validator.is_ok(), "Failed to create Azure validator");
    let validator = validator.expect("validator should be valid");
    assert_eq!(validator.provider_name(), "Azure Blob Storage");
}

/// Test simple workload execution.
#[tokio::test]
#[ignore = "requires Azure credentials and test container"]
async fn test_azure_simple_workload() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let container = get_azure_container().expect("TEST_AZURE_CONTAINER not set");
    let validator = AzureValidator::from_env(&container)
        .await
        .expect("Failed to create validator");

    let workload = ValidationWorkload {
        operations: simple_workload(),
        bucket: container,
        key_prefix: "test-simple/".to_string(),
        cleanup_after: true,
        timeout_seconds: 300,
    };

    let result = validator.execute_workload(&workload).await;
    assert!(result.is_ok(), "Workload execution failed: {result:?}");

    let execution = result.expect("execution should succeed");
    assert!(execution.operations_executed > 0);
}

/// Test storage workload validation.
#[tokio::test]
#[ignore = "requires Azure credentials and test container"]
async fn test_azure_storage_workload_validation() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let container = get_azure_container().expect("TEST_AZURE_CONTAINER not set");
    let validator = AzureValidator::from_env(&container)
        .await
        .expect("Failed to create validator");

    // Load Azure pricing rules
    let rules = pricing::load_rules("examples/azure-blob.toml").expect("Failed to load rules");

    let workload = ValidationWorkload {
        operations: storage_workload(5, 10 * 1024), // 5 files, 10 KB each
        bucket: container,
        key_prefix: "test-storage/".to_string(),
        cleanup_after: true,
        timeout_seconds: 300,
    };

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {result:?}");

    let validation = result.expect("validation should succeed");
    println!("Simulated cost: {}", validation.simulated_total);
    println!("Accuracy: {:.2}%", validation.accuracy());
}

/// Test operations workload validation.
#[tokio::test]
#[ignore = "requires Azure credentials and test container"]
async fn test_azure_operations_workload_validation() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let container = get_azure_container().expect("TEST_AZURE_CONTAINER not set");
    let validator = AzureValidator::from_env(&container)
        .await
        .expect("Failed to create validator");

    let rules = pricing::load_rules("examples/azure-blob.toml").expect("Failed to load rules");

    let workload = ValidationWorkload {
        operations: operations_workload(10, 20, 5),
        bucket: container,
        key_prefix: "test-ops/".to_string(),
        cleanup_after: true,
        timeout_seconds: 300,
    };

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {result:?}");

    let validation = result.expect("validation should succeed");
    println!("Simulated cost: {}", validation.simulated_total);
}

/// Test mixed workload validation.
#[tokio::test]
#[ignore = "requires Azure credentials and test container"]
async fn test_azure_mixed_workload_validation() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let container = get_azure_container().expect("TEST_AZURE_CONTAINER not set");
    let validator = AzureValidator::from_env(&container)
        .await
        .expect("Failed to create validator");

    let rules = pricing::load_rules("examples/azure-blob.toml").expect("Failed to load rules");

    let workload = ValidationWorkload {
        operations: mixed_workload(),
        bucket: container,
        key_prefix: "test-mixed/".to_string(),
        cleanup_after: true,
        timeout_seconds: 300,
    };

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {result:?}");

    let validation = result.expect("validation should succeed");
    println!("Simulated cost: {}", validation.simulated_total);
    println!("Categories validated: {:?}", validation.comparisons.len());
}

/// Test cleanup functionality.
#[tokio::test]
#[ignore = "requires Azure credentials and test container"]
async fn test_azure_cleanup() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let container = get_azure_container().expect("TEST_AZURE_CONTAINER not set");
    let validator = AzureValidator::from_env(&container)
        .await
        .expect("Failed to create validator");

    // Create some blobs
    let workload = ValidationWorkload {
        operations: simple_workload(),
        bucket: container,
        key_prefix: "test-cleanup/".to_string(),
        cleanup_after: false,
        timeout_seconds: 300,
    };

    let _ = validator.execute_workload(&workload).await;

    // Now cleanup
    let result = validator.cleanup().await;
    assert!(result.is_ok(), "Cleanup failed: {result:?}");
}

/// Test missing container error handling.
#[tokio::test]
#[ignore = "requires Azure credentials"]
async fn test_azure_missing_container() {
    if !azure_env_configured() {
        eprintln!("Skipping: Azure environment not configured");
        return;
    }

    let result = AzureValidator::from_env("nonexistent-container-xyz-12345").await;
    let Err(err) = result else {
        panic!("should fail for missing container")
    };

    // Should get a bucket not accessible error
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not accessible") || err_msg.contains("not found"),
        "Unexpected error: {err_msg}"
    );
}

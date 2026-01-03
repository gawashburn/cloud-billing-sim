//! B2 validation integration tests.
//!
//! These tests validate the billing simulator against real Backblaze B2.
//!
//! # Prerequisites
//!
//! - Environment variables:
//!   - `B2_APPLICATION_KEY_ID`: B2 application key ID
//!   - `B2_APPLICATION_KEY`: B2 application key
//!   - `TEST_B2_BUCKET`: Test bucket name
//!
//! # Running
//!
//! ```bash
//! cargo test --features b2-validation validation_b2 -- --ignored
//! ```

#![cfg(feature = "b2-validation")]
#![allow(clippy::expect_used)]

mod validation_harness;

use cloud_billing_sim::pricing;
use cloud_billing_sim::validation::b2::B2Validator;
use cloud_billing_sim::validation::{ValidationProvider, ValidationWorkload};
use validation_harness::{
    b2_env_configured, get_b2_bucket, mixed_workload, operations_workload, simple_workload,
    storage_workload,
};

/// Test B2 validator initialization.
#[tokio::test]
#[ignore = "requires B2 credentials and test bucket"]
async fn test_b2_validator_init() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let bucket = get_b2_bucket().expect("TEST_B2_BUCKET required");

    let validator = B2Validator::from_env(&bucket).await;
    assert!(
        validator.is_ok(),
        "Failed to create validator: {:?}",
        validator.err()
    );

    let validator = validator.expect("validator should be created");
    assert_eq!(validator.provider_name(), "Backblaze B2");
    assert_eq!(validator.region(), "global");
}

/// Test simple workload execution against B2.
#[tokio::test]
#[ignore = "requires B2 credentials and test bucket"]
async fn test_b2_simple_workload() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let bucket = get_b2_bucket().expect("TEST_B2_BUCKET required");

    let validator = B2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    let ops = simple_workload();
    let workload = ValidationWorkload::new(ops, &bucket)
        .with_prefix("test-simple/")
        .with_timeout(120);

    let result = validator.execute_workload(&workload).await;
    assert!(
        result.is_ok(),
        "Workload execution failed: {:?}",
        result.err()
    );

    let execution = result.expect("execution should succeed");
    assert!(execution.operations_executed > 0);
    assert!(execution.bytes_uploaded > 0);

    // Cleanup
    validator.cleanup().await.expect("cleanup should succeed");
}

/// Test storage workload validation.
#[tokio::test]
#[ignore = "requires B2 credentials and test bucket"]
async fn test_b2_storage_workload_validation() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let bucket = get_b2_bucket().expect("TEST_B2_BUCKET required");

    let validator = B2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    // Load B2 pricing rules
    let rules = pricing::load_rules("pricing/backblaze-b2.toml")
        .expect("failed to load B2 pricing rules");

    let ops = storage_workload(5, 1024 * 1024); // 5 x 1MB files
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-storage/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");

    // Simulated costs should be calculated
    assert!(validation.simulated_total.as_decimal() >= rust_decimal::Decimal::ZERO);
}

/// Test operations-heavy workload validation.
#[tokio::test]
#[ignore = "requires B2 credentials and test bucket"]
async fn test_b2_operations_workload_validation() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let bucket = get_b2_bucket().expect("TEST_B2_BUCKET required");

    let validator = B2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    let rules = pricing::load_rules("pricing/backblaze-b2.toml")
        .expect("failed to load B2 pricing rules");

    let ops = operations_workload(10, 50, 10); // 10 puts, 50 gets, 10 lists
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-ops/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");

    // B2 has free tier operations, costs may be $0
    assert!(validation.comparisons.len() >= 1);
}

/// Test mixed workload validation.
#[tokio::test]
#[ignore = "requires B2 credentials and test bucket"]
async fn test_b2_mixed_workload_validation() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let bucket = get_b2_bucket().expect("TEST_B2_BUCKET required");

    let validator = B2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    let rules = pricing::load_rules("pricing/backblaze-b2.toml")
        .expect("failed to load B2 pricing rules");

    // B2 only has one storage class, so use a simplified workload
    let ops = simple_workload();
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-mixed/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");
}

/// Test cleanup after workload.
#[tokio::test]
#[ignore = "requires B2 credentials and test bucket"]
async fn test_b2_cleanup() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let bucket = get_b2_bucket().expect("TEST_B2_BUCKET required");

    let validator = B2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    // Create some objects
    let ops = storage_workload(3, 1024);
    let workload = ValidationWorkload::new(ops, &bucket)
        .with_prefix("test-cleanup/")
        .keep_objects(); // Don't auto-cleanup

    let result = validator.execute_workload(&workload).await;
    assert!(result.is_ok());

    let execution = result.expect("execution should succeed");
    assert!(!execution.created_objects.is_empty());

    // Now cleanup
    let cleanup_result = validator.cleanup().await;
    assert!(cleanup_result.is_ok());
}

/// Test validator handles missing bucket.
#[tokio::test]
#[ignore = "requires B2 credentials"]
async fn test_b2_missing_bucket() {
    if !b2_env_configured() {
        eprintln!("Skipping: B2 environment variables not set");
        return;
    }

    let result = B2Validator::from_env("nonexistent-bucket-12345").await;
    assert!(result.is_err());

    let err = result.expect_err("should fail for missing bucket");
    assert!(matches!(
        err,
        cloud_billing_sim::validation::ValidationError::BucketNotAccessible(_)
    ));
}

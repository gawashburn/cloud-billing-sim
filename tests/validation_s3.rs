//! S3 validation integration tests.
//!
//! These tests validate the billing simulator against real AWS S3.
//!
//! # Prerequisites
//!
//! - AWS credentials configured
//! - Environment variables:
//!   - `TEST_S3_BUCKET`: Test bucket name
//!   - `TEST_S3_REGION`: AWS region (default: us-east-1)
//!
//! # Running
//!
//! ```bash
//! cargo test --features s3-validation validation_s3 -- --ignored
//! ```

#![cfg(feature = "s3-validation")]
#![allow(clippy::expect_used)]

mod validation_harness;

use cloud_billing_sim::pricing;
use cloud_billing_sim::validation::s3::S3Validator;
use cloud_billing_sim::validation::{ValidationProvider, ValidationWorkload};
use validation_harness::{
    get_s3_bucket, get_s3_region, mixed_workload, operations_workload, s3_env_configured,
    simple_workload, storage_workload,
};

/// Test S3 validator initialization.
#[tokio::test]
#[ignore = "requires AWS credentials and test bucket"]
async fn test_s3_validator_init() {
    if !s3_env_configured() {
        eprintln!("Skipping: TEST_S3_BUCKET not set");
        return;
    }

    let bucket = get_s3_bucket().expect("TEST_S3_BUCKET required");
    let region = get_s3_region();

    let validator = S3Validator::new(&bucket, &region).await;
    assert!(
        validator.is_ok(),
        "Failed to create validator: {:?}",
        validator.err()
    );

    let validator = validator.expect("validator should be created");
    assert_eq!(validator.provider_name(), "AWS S3");
    assert_eq!(validator.region(), region);
}

/// Test simple workload execution against S3.
#[tokio::test]
#[ignore = "requires AWS credentials and test bucket"]
async fn test_s3_simple_workload() {
    if !s3_env_configured() {
        eprintln!("Skipping: TEST_S3_BUCKET not set");
        return;
    }

    let bucket = get_s3_bucket().expect("TEST_S3_BUCKET required");
    let region = get_s3_region();

    let validator = S3Validator::new(&bucket, &region)
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
#[ignore = "requires AWS credentials and test bucket"]
async fn test_s3_storage_workload_validation() {
    if !s3_env_configured() {
        eprintln!("Skipping: TEST_S3_BUCKET not set");
        return;
    }

    let bucket = get_s3_bucket().expect("TEST_S3_BUCKET required");
    let region = get_s3_region();

    let validator = S3Validator::new(&bucket, &region)
        .await
        .expect("failed to create validator");

    // Load S3 pricing rules
    let pricing_path = format!("pricing/aws-s3-{region}.toml");
    let rules = pricing::load_rules(&pricing_path)
        .or_else(|_| pricing::load_rules("pricing/aws-s3-us-east-1.toml"))
        .expect("failed to load pricing rules");

    let ops = storage_workload(5, 1024 * 1024); // 5 x 1MB files
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-storage/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");

    // Note: actual costs may not be available immediately
    assert!(validation.simulated_total.as_decimal() > rust_decimal::Decimal::ZERO);
}

/// Test operations-heavy workload validation.
#[tokio::test]
#[ignore = "requires AWS credentials and test bucket"]
async fn test_s3_operations_workload_validation() {
    if !s3_env_configured() {
        eprintln!("Skipping: TEST_S3_BUCKET not set");
        return;
    }

    let bucket = get_s3_bucket().expect("TEST_S3_BUCKET required");
    let region = get_s3_region();

    let validator = S3Validator::new(&bucket, &region)
        .await
        .expect("failed to create validator");

    let pricing_path = format!("pricing/aws-s3-{region}.toml");
    let rules = pricing::load_rules(&pricing_path)
        .or_else(|_| pricing::load_rules("pricing/aws-s3-us-east-1.toml"))
        .expect("failed to load pricing rules");

    let ops = operations_workload(10, 50, 10); // 10 puts, 50 gets, 10 lists
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-ops/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");

    // Check that operation costs were calculated
    assert!(
        validation
            .comparisons
            .iter()
            .any(|c| c.category == "operations")
    );
}

/// Test mixed workload validation.
#[tokio::test]
#[ignore = "requires AWS credentials and test bucket"]
async fn test_s3_mixed_workload_validation() {
    if !s3_env_configured() {
        eprintln!("Skipping: TEST_S3_BUCKET not set");
        return;
    }

    let bucket = get_s3_bucket().expect("TEST_S3_BUCKET required");
    let region = get_s3_region();

    let validator = S3Validator::new(&bucket, &region)
        .await
        .expect("failed to create validator");

    let pricing_path = format!("pricing/aws-s3-{region}.toml");
    let rules = pricing::load_rules(&pricing_path)
        .or_else(|_| pricing::load_rules("pricing/aws-s3-us-east-1.toml"))
        .expect("failed to load pricing rules");

    let ops = mixed_workload();
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-mixed/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");

    // Verify we have category breakdowns
    assert!(validation.comparisons.len() >= 3);
}

/// Test cleanup after workload.
#[tokio::test]
#[ignore = "requires AWS credentials and test bucket"]
async fn test_s3_cleanup() {
    if !s3_env_configured() {
        eprintln!("Skipping: TEST_S3_BUCKET not set");
        return;
    }

    let bucket = get_s3_bucket().expect("TEST_S3_BUCKET required");
    let region = get_s3_region();

    let validator = S3Validator::new(&bucket, &region)
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

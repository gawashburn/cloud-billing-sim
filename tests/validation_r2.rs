//! R2 validation integration tests.
//!
//! These tests validate the billing simulator against real Cloudflare R2.
//!
//! # Prerequisites
//!
//! - Environment variables:
//!   - `R2_ACCOUNT_ID`: Cloudflare account ID
//!   - `R2_ACCESS_KEY_ID`: R2 access key ID
//!   - `R2_SECRET_ACCESS_KEY`: R2 secret access key
//!   - `TEST_R2_BUCKET`: Test bucket name
//!
//! # Running
//!
//! ```bash
//! cargo test --features r2-validation validation_r2 -- --ignored
//! ```

#![cfg(feature = "r2-validation")]
#![allow(clippy::expect_used)]

mod validation_harness;

use cloud_billing_sim::pricing;
use cloud_billing_sim::validation::r2::R2Validator;
use cloud_billing_sim::validation::{ValidationProvider, ValidationWorkload};
use validation_harness::{
    get_r2_bucket, operations_workload, r2_env_configured, simple_workload, storage_workload,
};

/// Test R2 validator initialization.
#[tokio::test]
#[ignore = "requires R2 credentials and test bucket"]
async fn test_r2_validator_init() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let bucket = get_r2_bucket().expect("TEST_R2_BUCKET required");

    let validator = R2Validator::from_env(&bucket).await;
    assert!(
        validator.is_ok(),
        "Failed to create validator: {:?}",
        validator.err()
    );

    let validator = validator.expect("validator should be created");
    assert_eq!(validator.provider_name(), "Cloudflare R2");
    assert_eq!(validator.region(), "global");
}

/// Test simple workload execution against R2.
#[tokio::test]
#[ignore = "requires R2 credentials and test bucket"]
async fn test_r2_simple_workload() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let bucket = get_r2_bucket().expect("TEST_R2_BUCKET required");

    let validator = R2Validator::from_env(&bucket)
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
#[ignore = "requires R2 credentials and test bucket"]
async fn test_r2_storage_workload_validation() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let bucket = get_r2_bucket().expect("TEST_R2_BUCKET required");

    let validator = R2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    // Load R2 pricing rules
    let rules = pricing::load_rules("examples/cloudflare-r2.toml")
        .expect("failed to load R2 pricing rules");

    let ops = storage_workload(5, 1024 * 1024); // 5 x 1MB files
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-storage/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");
    println!("{validation}");

    // Simulated costs should be calculated
    assert!(validation.simulated_total.as_decimal() >= rust_decimal::Decimal::ZERO);

    // R2 has zero egress - check that data transfer cost is zero
    let transfer_comp = validation
        .comparisons
        .iter()
        .find(|c| c.category == "data_transfer");
    if let Some(comp) = transfer_comp {
        assert!(
            comp.simulated.is_zero(),
            "R2 should have zero egress costs"
        );
    }
}

/// Test operations-heavy workload validation.
#[tokio::test]
#[ignore = "requires R2 credentials and test bucket"]
async fn test_r2_operations_workload_validation() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let bucket = get_r2_bucket().expect("TEST_R2_BUCKET required");

    let validator = R2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    let rules = pricing::load_rules("examples/cloudflare-r2.toml")
        .expect("failed to load R2 pricing rules");

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

/// Test cleanup after workload.
#[tokio::test]
#[ignore = "requires R2 credentials and test bucket"]
async fn test_r2_cleanup() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let bucket = get_r2_bucket().expect("TEST_R2_BUCKET required");

    let validator = R2Validator::from_env(&bucket)
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
#[ignore = "requires R2 credentials"]
async fn test_r2_missing_bucket() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let result = R2Validator::from_env("nonexistent-bucket-12345").await;
    assert!(result.is_err());

    let err = result.expect_err("should fail for missing bucket");
    assert!(matches!(
        err,
        cloud_billing_sim::validation::ValidationError::BucketNotAccessible(_)
    ));
}

/// Test that R2's zero egress pricing is correctly applied.
#[tokio::test]
#[ignore = "requires R2 credentials and test bucket"]
async fn test_r2_zero_egress() {
    if !r2_env_configured() {
        eprintln!("Skipping: R2 environment variables not set");
        return;
    }

    let bucket = get_r2_bucket().expect("TEST_R2_BUCKET required");

    let validator = R2Validator::from_env(&bucket)
        .await
        .expect("failed to create validator");

    let rules = pricing::load_rules("examples/cloudflare-r2.toml")
        .expect("failed to load R2 pricing rules");

    // Create a workload with significant data transfer
    let ops = simple_workload();
    let workload = ValidationWorkload::new(ops, &bucket).with_prefix("test-egress/");

    let result = validator.validate_workload(&workload, &rules).await;
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());

    let validation = result.expect("validation should succeed");

    // Find data transfer comparison
    let transfer_comp = validation
        .comparisons
        .iter()
        .find(|c| c.category == "data_transfer");

    if let Some(comp) = transfer_comp {
        // R2 has zero egress, so simulated data transfer should be $0
        assert!(
            comp.simulated.is_zero(),
            "R2 egress should be free, but got: {}",
            comp.simulated
        );
    }

    // Check warnings mention zero egress
    assert!(
        validation
            .warnings
            .iter()
            .any(|w| w.contains("zero egress")),
        "Should have warning about zero egress"
    );
}

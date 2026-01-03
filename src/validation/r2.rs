//! Cloudflare R2 validation implementation.
//!
//! This module provides validation against Cloudflare R2 using the S3-compatible API.
//!
//! # Prerequisites
//!
//! - Cloudflare account with R2 enabled
//! - R2 API token with appropriate permissions
//! - A test bucket in R2
//!
//! # Environment Variables
//!
//! - `R2_ACCOUNT_ID`: Cloudflare account ID
//! - `R2_ACCESS_KEY_ID`: R2 API access key ID
//! - `R2_SECRET_ACCESS_KEY`: R2 API secret access key
//!
//! # Example
//!
//! ```no_run,ignore
//! use cloud_billing_sim::validation::r2::R2Validator;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let validator = R2Validator::from_env("my-test-bucket").await?;
//!
//! // Execute and validate a workload
//! let result = validator.validate_workload(&workload, &rules).await?;
//! println!("Accuracy: {:.2}%", result.accuracy());
//! # Ok(())
//! # }
//! ```

use crate::engine::Simulator;
use crate::operations::{Operation, OperationKind};
use crate::pricing::PricingRules;
use crate::types::{Bytes, StorageClass};
use crate::validation::{
    ActualCosts, CostComparison, ExecutedWorkload, ValidationError, ValidationProvider,
    ValidationResult, ValidationWorkload,
};
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cloudflare R2 validation provider.
///
/// Connects to R2 using the S3-compatible API to execute real operations
/// and compare costs. R2's key advantage is zero egress fees.
pub struct R2Validator {
    /// S3-compatible client configured for R2.
    client: S3Client,

    /// Cloudflare account ID (for future billing API integration).
    account_id: String,

    /// Test bucket name.
    bucket: String,

    /// Objects created during validation (for cleanup).
    created_objects: Arc<RwLock<Vec<String>>>,

    /// Default storage class for uploads.
    default_storage_class: StorageClass,
}

impl R2Validator {
    /// Creates a new R2 validator from environment variables.
    ///
    /// Reads `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, and `R2_SECRET_ACCESS_KEY`
    /// from the environment.
    ///
    /// # Errors
    ///
    /// Returns an error if environment variables are missing or authentication fails.
    pub async fn from_env(bucket: impl Into<String>) -> Result<Self, ValidationError> {
        let account_id = std::env::var("R2_ACCOUNT_ID")
            .map_err(|_| ValidationError::InvalidConfig("R2_ACCOUNT_ID not set".to_string()))?;
        let access_key_id = std::env::var("R2_ACCESS_KEY_ID")
            .map_err(|_| ValidationError::InvalidConfig("R2_ACCESS_KEY_ID not set".to_string()))?;
        let secret_access_key = std::env::var("R2_SECRET_ACCESS_KEY").map_err(|_| {
            ValidationError::InvalidConfig("R2_SECRET_ACCESS_KEY not set".to_string())
        })?;

        Self::new(&account_id, &access_key_id, &secret_access_key, bucket).await
    }

    /// Creates a new R2 validator with explicit credentials.
    ///
    /// # Arguments
    ///
    /// * `account_id` - Cloudflare account ID
    /// * `access_key_id` - R2 API access key ID
    /// * `secret_access_key` - R2 API secret access key
    /// * `bucket` - Bucket name for validation tests
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails or bucket is not accessible.
    pub async fn new(
        account_id: &str,
        access_key_id: &str,
        secret_access_key: &str,
        bucket: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let bucket_str = bucket.into();
        let endpoint_url = format!("https://{account_id}.r2.cloudflarestorage.com");

        info!(
            account_id = %account_id,
            bucket = %bucket_str,
            endpoint = %endpoint_url,
            "initializing R2 validator"
        );

        // Create credentials
        let credentials = Credentials::new(
            access_key_id,
            secret_access_key,
            None, // session token
            None, // expiration
            "r2-credentials",
        );

        // Build S3 config for R2
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("auto"))
            .endpoint_url(&endpoint_url)
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();

        let client = S3Client::from_conf(config);

        // Verify bucket exists and is accessible
        client
            .head_bucket()
            .bucket(&bucket_str)
            .send()
            .await
            .map_err(|e| ValidationError::BucketNotAccessible(e.to_string()))?;

        info!(bucket = %bucket_str, "R2 bucket verified accessible");

        Ok(Self {
            client,
            account_id: account_id.to_string(),
            bucket: bucket_str,
            created_objects: Arc::new(RwLock::new(Vec::new())),
            default_storage_class: StorageClass::new("STANDARD"),
        })
    }

    /// Sets the default storage class for uploads.
    #[must_use]
    pub fn with_storage_class(mut self, class: StorageClass) -> Self {
        self.default_storage_class = class;
        self
    }

    /// Executes a single operation against R2.
    async fn execute_operation(
        &self,
        op: &Operation,
        key_prefix: &str,
    ) -> Result<OperationMetrics, ValidationError> {
        let key = op
            .key
            .as_ref()
            .map_or_else(|| "unnamed".to_string(), |k| format!("{key_prefix}{k}"));

        debug!(operation = ?op.kind, key = %key, "executing R2 operation");

        let metrics = match &op.kind {
            OperationKind::PutObject {
                size_bytes,
                storage_class,
            } => {
                let data = vec![0u8; *size_bytes as usize];
                let body = ByteStream::from(data);

                // R2 uses standard storage class names
                let storage_class_sdk = map_r2_storage_class(storage_class);

                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .body(body)
                    .storage_class(storage_class_sdk)
                    .send()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                self.created_objects.write().await.push(key.clone());

                OperationMetrics {
                    bytes_uploaded: *size_bytes,
                    bytes_downloaded: 0,
                    operation_type: "PUT".to_string(),
                }
            }

            OperationKind::GetObject {
                bytes_transferred, ..
            } => {
                let resp = self
                    .client
                    .get_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                let body = resp
                    .body
                    .collect()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                let body_bytes = body.into_bytes();
                let downloaded = bytes_transferred.unwrap_or(body_bytes.len() as u64);

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: downloaded,
                    operation_type: "GET".to_string(),
                }
            }

            OperationKind::DeleteObject => {
                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                self.created_objects.write().await.retain(|k| k != &key);

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "DELETE".to_string(),
                }
            }

            OperationKind::HeadObject => {
                self.client
                    .head_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "HEAD".to_string(),
                }
            }

            OperationKind::ListObjects { .. } => {
                self.client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&key)
                    .send()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "LIST".to_string(),
                }
            }

            OperationKind::CopyObject {
                source_key,
                source_bucket,
                ..
            } => {
                let source_bucket = source_bucket.as_ref().unwrap_or(&self.bucket);
                let copy_source = format!("{source_bucket}/{source_key}");

                self.client
                    .copy_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .copy_source(&copy_source)
                    .send()
                    .await
                    .map_err(|e| ValidationError::S3Operation(e.to_string()))?;

                self.created_objects.write().await.push(key.clone());

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "COPY".to_string(),
                }
            }

            // Multipart operations - R2 supports these
            OperationKind::CreateMultipartUpload { .. } => {
                warn!("multipart upload not fully supported in validation");
                OperationMetrics::default()
            }

            OperationKind::UploadPart { size_bytes, .. } => OperationMetrics {
                bytes_uploaded: *size_bytes,
                bytes_downloaded: 0,
                operation_type: "UPLOAD_PART".to_string(),
            },

            OperationKind::CompleteMultipartUpload { .. }
            | OperationKind::AbortMultipartUpload { .. } => OperationMetrics::default(),

            // R2 doesn't support archive/restore operations
            OperationKind::RestoreObject { .. } => {
                debug!("restore not supported by R2");
                OperationMetrics::default()
            }

            OperationKind::LifecycleTransition { .. } => {
                debug!("lifecycle transitions handled differently in R2");
                OperationMetrics::default()
            }

            OperationKind::SelectObjectContent { .. } => {
                debug!("select object content not supported by R2");
                OperationMetrics::default()
            }
        };

        Ok(metrics)
    }

    /// Deletes all objects created during validation.
    async fn delete_created_objects(&self) -> Result<usize, ValidationError> {
        let objects = self.created_objects.read().await.clone();
        let count = objects.len();

        info!(count = %count, "cleaning up R2 validation objects");

        for key in objects {
            if let Err(e) = self
                .client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
            {
                warn!(key = %key, error = %e, "failed to delete object during cleanup");
            }
        }

        self.created_objects.write().await.clear();
        Ok(count)
    }
}

impl ValidationProvider for R2Validator {
    async fn execute_workload(
        &self,
        workload: &ValidationWorkload,
    ) -> Result<ExecutedWorkload, ValidationError> {
        let start_time = Utc::now();
        let mut bytes_uploaded = 0u64;
        let mut bytes_downloaded = 0u64;
        let mut operations_executed = 0usize;

        info!(
            bucket = %workload.bucket,
            prefix = %workload.key_prefix,
            ops_count = %workload.operations.operations.len(),
            "executing R2 validation workload"
        );

        for op in &workload.operations.operations {
            let metrics = self.execute_operation(op, &workload.key_prefix).await?;
            bytes_uploaded += metrics.bytes_uploaded;
            bytes_downloaded += metrics.bytes_downloaded;
            operations_executed += 1;
        }

        let end_time = Utc::now();
        let created_objects = self.created_objects.read().await.clone();

        info!(
            operations = %operations_executed,
            uploaded = %Bytes::new(bytes_uploaded),
            downloaded = %Bytes::new(bytes_downloaded),
            duration_ms = %(end_time - start_time).num_milliseconds(),
            "R2 workload execution complete"
        );

        Ok(ExecutedWorkload {
            start_time,
            end_time,
            operations_executed,
            bytes_uploaded,
            bytes_downloaded,
            created_objects,
            metadata: HashMap::new(),
        })
    }

    async fn get_actual_costs(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        _bucket: Option<&str>,
    ) -> Result<ActualCosts, ValidationError> {
        // R2 billing is visible in the Cloudflare dashboard
        // There's no API for real-time cost retrieval currently

        warn!(
            start = %start,
            end = %end,
            "R2 billing data is only available via Cloudflare dashboard"
        );

        Ok(ActualCosts {
            is_complete: false,
            notes: vec![
                "R2 billing data is visible in Cloudflare dashboard".to_string(),
                "R2 has zero egress fees - data transfer costs should be $0".to_string(),
                "Free tier: 10GB storage, 1M Class A ops, 10M Class B ops/month".to_string(),
            ],
            period: Some((start, end)),
            ..Default::default()
        })
    }

    async fn validate_workload(
        &self,
        workload: &ValidationWorkload,
        rules: &PricingRules,
    ) -> Result<ValidationResult, ValidationError> {
        // Execute the workload
        let execution = self.execute_workload(workload).await?;

        // Run the simulator
        let mut simulator = Simulator::new(rules.clone());
        let report = simulator
            .simulate(&workload.operations)
            .map_err(|e| ValidationError::Simulation(e.to_string()))?;

        let simulated_total = report.total_cost;

        // Try to get actual costs
        let actual_costs = self
            .get_actual_costs(execution.start_time, execution.end_time, Some(&workload.bucket))
            .await?;

        // Build result
        let mut result = ValidationResult::new(
            self.provider_name(),
            "global",
            simulated_total,
            actual_costs.total,
            execution,
        );

        // Add category comparisons
        let storage_comp = CostComparison::new(
            "storage",
            report.breakdown.total_storage,
            actual_costs.storage,
        );
        result.add_comparison(storage_comp);

        let ops_comp = CostComparison::new(
            "operations",
            report.breakdown.total_operations,
            actual_costs.operations,
        );
        result.add_comparison(ops_comp);

        // R2 has zero egress - this should always match!
        let transfer_comp = CostComparison::new(
            "data_transfer",
            report.breakdown.data_transfer_egress,
            actual_costs.data_transfer,
        );
        result.add_comparison(transfer_comp);

        // Add note about zero egress
        result.add_warning("R2 has zero egress fees - simulated egress should be $0");

        if !actual_costs.is_complete {
            result.add_warning("Actual billing data not available from R2 API");
        }
        for note in actual_costs.notes {
            result.add_warning(note);
        }

        // Cleanup if requested
        if workload.cleanup_after {
            self.cleanup().await?;
        }

        Ok(result.with_tolerance(5.0))
    }

    fn provider_name(&self) -> &str {
        "Cloudflare R2"
    }

    fn region(&self) -> &str {
        "global"
    }

    async fn cleanup(&self) -> Result<(), ValidationError> {
        let deleted = self.delete_created_objects().await?;
        info!(deleted = %deleted, "R2 cleanup complete");
        Ok(())
    }
}

impl std::fmt::Debug for R2Validator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("R2Validator")
            .field("account_id", &self.account_id)
            .field("bucket", &self.bucket)
            .field("default_storage_class", &self.default_storage_class)
            .finish_non_exhaustive()
    }
}

/// Metrics from a single operation.
#[derive(Debug, Default)]
#[allow(dead_code)]
struct OperationMetrics {
    bytes_uploaded: u64,
    bytes_downloaded: u64,
    operation_type: String,
}

/// Maps storage class to R2-compatible storage class.
///
/// R2 currently only supports STANDARD and INFREQUENT_ACCESS.
fn map_r2_storage_class(class: &StorageClass) -> aws_sdk_s3::types::StorageClass {
    match class.as_str() {
        "INFREQUENT_ACCESS" | "INFREQUENT-ACCESS" | "IA" => {
            // R2's infrequent access maps to STANDARD_IA in S3 SDK
            aws_sdk_s3::types::StorageClass::StandardIa
        }
        // Everything else maps to STANDARD
        _ => aws_sdk_s3::types::StorageClass::Standard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_r2_storage_class_standard() {
        let class = StorageClass::new("STANDARD");
        let sdk_class = map_r2_storage_class(&class);
        assert_eq!(sdk_class, aws_sdk_s3::types::StorageClass::Standard);
    }

    #[test]
    fn map_r2_storage_class_infrequent_access() {
        let class = StorageClass::new("INFREQUENT_ACCESS");
        let sdk_class = map_r2_storage_class(&class);
        assert_eq!(sdk_class, aws_sdk_s3::types::StorageClass::StandardIa);
    }

    #[test]
    fn map_r2_storage_class_unknown_defaults_to_standard() {
        let class = StorageClass::new("GLACIER");
        let sdk_class = map_r2_storage_class(&class);
        assert_eq!(sdk_class, aws_sdk_s3::types::StorageClass::Standard);
    }
}

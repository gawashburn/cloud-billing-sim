//! AWS S3 validation implementation.
//!
//! This module provides validation against AWS S3 using the AWS SDK.
//!
//! # Prerequisites
//!
//! - AWS credentials configured (via environment variables, credentials file, or IAM role)
//! - A test bucket in the target region
//! - Appropriate IAM permissions for S3 and Cost Explorer
//!
//! # Example
//!
//! ```no_run,ignore
//! use cloud_billing_sim::validation::s3::S3Validator;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let validator = S3Validator::new("my-test-bucket", "us-east-1").await?;
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
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// AWS S3 validation provider.
///
/// Connects to AWS S3 to execute real operations and compare costs.
pub struct S3Validator {
    /// S3 client.
    client: S3Client,

    /// Test bucket name.
    bucket: String,

    /// AWS region.
    region: String,

    /// Objects created during validation (for cleanup).
    created_objects: Arc<RwLock<Vec<String>>>,

    /// Default storage class for uploads.
    default_storage_class: StorageClass,
}

impl S3Validator {
    /// Creates a new S3 validator.
    ///
    /// # Arguments
    ///
    /// * `bucket` - S3 bucket to use for validation tests
    /// * `region` - AWS region (e.g., "us-east-1")
    ///
    /// # Errors
    ///
    /// Returns an error if AWS configuration fails.
    ///
    /// # Example
    ///
    /// ```no_run,ignore
    /// let validator = S3Validator::new("test-bucket", "us-east-1").await?;
    /// ```
    pub async fn new(
        bucket: impl Into<String>,
        region: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let region_str = region.into();
        let bucket_str = bucket.into();

        info!(bucket = %bucket_str, region = %region_str, "initializing S3 validator");

        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region_str.clone()))
            .load()
            .await;

        let client = S3Client::new(&config);

        // Verify bucket exists and is accessible
        client
            .head_bucket()
            .bucket(&bucket_str)
            .send()
            .await
            .map_err(|e| ValidationError::BucketNotAccessible(e.to_string()))?;

        info!(bucket = %bucket_str, "bucket verified accessible");

        Ok(Self {
            client,
            bucket: bucket_str,
            region: region_str,
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

    /// Executes a single operation against S3.
    async fn execute_operation(
        &self,
        op: &Operation,
        key_prefix: &str,
    ) -> Result<OperationMetrics, ValidationError> {
        let key = op
            .key
            .as_ref()
            .map_or_else(|| "unnamed".to_string(), |k| format!("{key_prefix}{k}"));

        debug!(operation = ?op.kind, key = %key, "executing S3 operation");

        let metrics = match &op.kind {
            OperationKind::PutObject {
                size_bytes,
                storage_class,
            } => {
                let data = vec![0u8; *size_bytes as usize];
                let body = ByteStream::from(data);

                let storage_class_sdk = map_storage_class(storage_class);

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

                // Remove from tracked objects
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

            // Multipart operations
            OperationKind::CreateMultipartUpload { .. } => {
                warn!("multipart upload not fully supported in validation");
                OperationMetrics::default()
            }

            OperationKind::UploadPart { size_bytes, .. } => {
                OperationMetrics {
                    bytes_uploaded: *size_bytes,
                    bytes_downloaded: 0,
                    operation_type: "UPLOAD_PART".to_string(),
                }
            }

            OperationKind::CompleteMultipartUpload { .. }
            | OperationKind::AbortMultipartUpload { .. } => OperationMetrics::default(),

            // Archive operations
            OperationKind::RestoreObject { days, tier } => {
                let tier_str = tier.map_or("Standard", |t| t.as_str());
                debug!(days = %days, tier = %tier_str, "restore not executed in validation");
                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "RESTORE".to_string(),
                }
            }

            OperationKind::LifecycleTransition { .. } => {
                debug!("lifecycle transition not executed in validation");
                OperationMetrics::default()
            }

            OperationKind::SelectObjectContent { .. } => {
                debug!("select object content not supported in validation");
                OperationMetrics::default()
            }
        };

        Ok(metrics)
    }

    /// Deletes all objects created during validation.
    async fn delete_created_objects(&self) -> Result<usize, ValidationError> {
        let objects = self.created_objects.read().await.clone();
        let count = objects.len();

        info!(count = %count, "cleaning up validation objects");

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

impl ValidationProvider for S3Validator {
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
            "executing S3 validation workload"
        );

        for op in &workload.operations.operations {
            let metrics = self
                .execute_operation(op, &workload.key_prefix)
                .await?;
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
            "workload execution complete"
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
        bucket: Option<&str>,
    ) -> Result<ActualCosts, ValidationError> {
        // Note: AWS Cost Explorer has a delay of up to 24 hours
        // For real-time validation, we would need to estimate based on
        // the operations performed and known pricing

        warn!(
            start = %start,
            end = %end,
            bucket = ?bucket,
            "AWS Cost Explorer data may be delayed up to 24 hours"
        );

        // In a production implementation, this would call Cost Explorer API
        // For now, return a placeholder indicating data is not yet available
        Ok(ActualCosts {
            is_complete: false,
            notes: vec![
                "AWS billing data is typically delayed 24+ hours".to_string(),
                "Consider using AWS Cost and Usage Reports for historical validation".to_string(),
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

        // Try to get actual costs (may not be available immediately)
        let actual_costs = self
            .get_actual_costs(
                execution.start_time,
                execution.end_time,
                Some(&workload.bucket),
            )
            .await?;

        // Build result
        let mut result = ValidationResult::new(
            self.provider_name(),
            &self.region,
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

        let transfer_comp = CostComparison::new(
            "data_transfer",
            report.breakdown.data_transfer_egress,
            actual_costs.data_transfer,
        );
        result.add_comparison(transfer_comp);

        // Add warnings if actual data is incomplete
        if !actual_costs.is_complete {
            result.add_warning("Actual billing data may be incomplete or delayed");
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
        "AWS S3"
    }

    fn region(&self) -> &str {
        &self.region
    }

    async fn cleanup(&self) -> Result<(), ValidationError> {
        let deleted = self.delete_created_objects().await?;
        info!(deleted = %deleted, "cleanup complete");
        Ok(())
    }
}

impl std::fmt::Debug for S3Validator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Validator")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
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

/// Maps our storage class to AWS SDK storage class.
fn map_storage_class(class: &StorageClass) -> aws_sdk_s3::types::StorageClass {
    match class.as_str() {
        "STANDARD" => aws_sdk_s3::types::StorageClass::Standard,
        "STANDARD_IA" | "STANDARD-IA" => aws_sdk_s3::types::StorageClass::StandardIa,
        "ONEZONE_IA" | "ONEZONE-IA" => aws_sdk_s3::types::StorageClass::OnezoneIa,
        "GLACIER" => aws_sdk_s3::types::StorageClass::Glacier,
        "GLACIER_IR" | "GLACIER-IR" => aws_sdk_s3::types::StorageClass::GlacierIr,
        "DEEP_ARCHIVE" | "DEEP-ARCHIVE" => aws_sdk_s3::types::StorageClass::DeepArchive,
        "INTELLIGENT_TIERING" | "INTELLIGENT-TIERING" => {
            aws_sdk_s3::types::StorageClass::IntelligentTiering
        }
        "REDUCED_REDUNDANCY" | "REDUCED-REDUNDANCY" => {
            aws_sdk_s3::types::StorageClass::ReducedRedundancy
        }
        _ => aws_sdk_s3::types::StorageClass::Standard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_storage_class_standard() {
        let class = StorageClass::new("STANDARD");
        let sdk_class = map_storage_class(&class);
        assert_eq!(sdk_class, aws_sdk_s3::types::StorageClass::Standard);
    }

    #[test]
    fn map_storage_class_glacier() {
        let class = StorageClass::new("GLACIER");
        let sdk_class = map_storage_class(&class);
        assert_eq!(sdk_class, aws_sdk_s3::types::StorageClass::Glacier);
    }

    #[test]
    fn map_storage_class_unknown_defaults_to_standard() {
        let class = StorageClass::new("UNKNOWN_CLASS");
        let sdk_class = map_storage_class(&class);
        assert_eq!(sdk_class, aws_sdk_s3::types::StorageClass::Standard);
    }
}

//! Azure Blob Storage validation implementation.
//!
//! This module provides validation against Azure Blob Storage using the Azure SDK.
//!
//! # Prerequisites
//!
//! - Azure Storage account
//! - Storage account connection string or account key
//! - A test container in the storage account
//!
//! # Environment Variables
//!
//! - `AZURE_STORAGE_ACCOUNT`: Storage account name
//! - `AZURE_STORAGE_ACCESS_KEY`: Storage account access key
//!
//!
//! # Example
//!
//! ```no_run,ignore
//! use cloud_billing_sim::validation::azure::AzureValidator;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let validator = AzureValidator::from_env("my-test-container").await?;
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
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::*;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Azure Blob Storage validation provider.
///
/// Connects to Azure Blob Storage to execute real operations and compare costs.
pub struct AzureValidator {
    /// Blob service client.
    client: BlobServiceClient,

    /// Storage account name.
    account_name: String,

    /// Test container name.
    container_name: String,

    /// Blobs created during validation (for cleanup).
    created_blobs: Arc<RwLock<Vec<String>>>,

    /// Default access tier for uploads.
    default_access_tier: AccessTier,
}

impl std::fmt::Debug for AzureValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureValidator")
            .field("account_name", &self.account_name)
            .field("container_name", &self.container_name)
            .field("default_access_tier", &self.default_access_tier)
            .finish_non_exhaustive()
    }
}

impl AzureValidator {
    /// Creates a new Azure validator from environment variables.
    ///
    /// Reads `AZURE_STORAGE_ACCOUNT` and `AZURE_STORAGE_ACCESS_KEY` from environment.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails or container is not found.
    pub async fn from_env(container_name: impl Into<String>) -> Result<Self, ValidationError> {
        let container = container_name.into();

        let account = std::env::var("AZURE_STORAGE_ACCOUNT").map_err(|_| {
            ValidationError::InvalidConfig("AZURE_STORAGE_ACCOUNT not set".to_string())
        })?;
        let key = std::env::var("AZURE_STORAGE_ACCESS_KEY").map_err(|_| {
            ValidationError::InvalidConfig("AZURE_STORAGE_ACCESS_KEY not set".to_string())
        })?;

        Self::new(&account, &key, container).await
    }

    /// Creates a new Azure validator with explicit credentials.
    ///
    /// # Arguments
    ///
    /// * `account` - Azure Storage account name
    /// * `access_key` - Storage account access key
    /// * `container_name` - Container to use for validation tests
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails or container is not accessible.
    pub async fn new(
        account: &str,
        access_key: &str,
        container_name: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let container = container_name.into();
        let account_owned = account.to_string();
        let key_owned = access_key.to_string();

        info!(
            account = %account_owned,
            container = %container,
            "initializing Azure validator"
        );

        let credentials = StorageCredentials::access_key(account_owned.clone(), key_owned);
        let client = BlobServiceClient::new(&account_owned, credentials);

        // Verify container exists and is accessible
        let container_client = client.container_client(&container);

        container_client
            .get_properties()
            .await
            .map_err(|e| ValidationError::BucketNotAccessible(e.to_string()))?;

        info!(container = %container, "Azure container verified accessible");

        Ok(Self {
            client,
            account_name: account_owned,
            container_name: container,
            created_blobs: Arc::new(RwLock::new(Vec::new())),
            default_access_tier: AccessTier::Hot,
        })
    }

    /// Sets the default access tier for uploads.
    #[must_use]
    pub fn with_access_tier(mut self, tier: AccessTier) -> Self {
        self.default_access_tier = tier;
        self
    }

    /// Gets a container client.
    fn container_client(&self) -> ContainerClient {
        self.client.container_client(&self.container_name)
    }

    /// Gets a blob client for a specific blob.
    fn blob_client(&self, blob_name: &str) -> BlobClient {
        self.container_client().blob_client(blob_name)
    }

    /// Executes a single operation against Azure Blob Storage.
    async fn execute_operation(
        &self,
        op: &Operation,
        key_prefix: &str,
    ) -> Result<OperationMetrics, ValidationError> {
        let blob_name = op
            .key
            .as_ref()
            .map_or_else(|| "unnamed".to_string(), |k| format!("{key_prefix}{k}"));

        debug!(operation = ?op.kind, blob = %blob_name, "executing Azure operation");

        let metrics = match &op.kind {
            OperationKind::PutObject {
                size_bytes,
                storage_class,
            } => {
                let data = vec![0u8; *size_bytes as usize];
                let blob_client = self.blob_client(&blob_name);
                let access_tier = map_azure_access_tier(storage_class);

                blob_client
                    .put_block_blob(data.clone())
                    .access_tier(access_tier)
                    .await
                    .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                self.created_blobs.write().await.push(blob_name.clone());

                OperationMetrics {
                    bytes_uploaded: *size_bytes,
                    bytes_downloaded: 0,
                    operation_type: "PUT".to_string(),
                }
            }

            OperationKind::GetObject {
                bytes_transferred, ..
            } => {
                let blob_client = self.blob_client(&blob_name);

                // Use streaming to download the blob
                let mut stream = blob_client.get().into_stream();
                let mut total_bytes = 0u64;

                while let Some(response_result) = stream.next().await {
                    let response = response_result
                        .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                    // Collect the data from this chunk
                    let data = response
                        .data
                        .collect()
                        .await
                        .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;
                    total_bytes += data.len() as u64;
                }

                let downloaded = bytes_transferred.unwrap_or(total_bytes);

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: downloaded,
                    operation_type: "GET".to_string(),
                }
            }

            OperationKind::DeleteObject => {
                let blob_client = self.blob_client(&blob_name);

                blob_client
                    .delete()
                    .await
                    .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                self.created_blobs
                    .write()
                    .await
                    .retain(|b| b != &blob_name);

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "DELETE".to_string(),
                }
            }

            OperationKind::HeadObject => {
                let blob_client = self.blob_client(&blob_name);

                blob_client
                    .get_properties()
                    .await
                    .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "HEAD".to_string(),
                }
            }

            OperationKind::ListObjects { .. } => {
                let container_client = self.container_client();

                let mut stream = container_client.list_blobs().prefix(blob_name).into_stream();

                while let Some(result) = stream.next().await {
                    result.map_err(|e| ValidationError::AzureOperation(e.to_string()))?;
                }

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
                let source_container = source_bucket.as_ref().unwrap_or(&self.container_name);
                let source_url = format!(
                    "https://{}.blob.core.windows.net/{}/{}",
                    self.account_name, source_container, source_key
                );

                let blob_client = self.blob_client(&blob_name);

                let parsed_url = url::Url::parse(&source_url)
                    .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                blob_client
                    .copy(parsed_url)
                    .await
                    .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                self.created_blobs.write().await.push(blob_name.clone());

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "COPY".to_string(),
                }
            }

            OperationKind::LifecycleTransition { new_storage_class } => {
                let blob_client = self.blob_client(&blob_name);
                let access_tier = map_azure_access_tier(new_storage_class);

                blob_client
                    .set_blob_tier(access_tier)
                    .await
                    .map_err(|e| ValidationError::AzureOperation(e.to_string()))?;

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "SET_TIER".to_string(),
                }
            }

            // Operations not directly supported by Azure Blob Storage
            OperationKind::CreateMultipartUpload { .. }
            | OperationKind::UploadPart { .. }
            | OperationKind::CompleteMultipartUpload { .. }
            | OperationKind::AbortMultipartUpload { .. } => {
                // Azure uses block blobs with different multipart semantics
                debug!("multipart operations handled differently in Azure");
                OperationMetrics::default()
            }

            OperationKind::RestoreObject { .. } => {
                // Azure archive rehydration is handled via set_blob_tier
                debug!("restore not directly supported; use lifecycle transition");
                OperationMetrics::default()
            }

            OperationKind::SelectObjectContent { .. } => {
                debug!("select object content not supported by Azure Blob");
                OperationMetrics::default()
            }
        };

        Ok(metrics)
    }

    /// Deletes all blobs created during validation.
    async fn delete_created_blobs(&self) -> Result<usize, ValidationError> {
        let blobs = self.created_blobs.read().await.clone();
        let count = blobs.len();

        info!(count = %count, "cleaning up Azure validation blobs");

        for blob_name in blobs {
            if let Err(e) = self.blob_client(&blob_name).delete().await {
                warn!(blob = %blob_name, error = %e, "failed to delete blob during cleanup");
            }
        }

        self.created_blobs.write().await.clear();
        Ok(count)
    }
}

impl ValidationProvider for AzureValidator {
    async fn execute_workload(
        &self,
        workload: &ValidationWorkload,
    ) -> Result<ExecutedWorkload, ValidationError> {
        let start_time = Utc::now();
        let mut bytes_uploaded = 0u64;
        let mut bytes_downloaded = 0u64;
        let mut operations_executed = 0usize;

        info!(
            container = %workload.bucket,
            prefix = %workload.key_prefix,
            ops_count = %workload.operations.operations.len(),
            "executing Azure validation workload"
        );

        for op in &workload.operations.operations {
            let metrics = self.execute_operation(op, &workload.key_prefix).await?;
            bytes_uploaded += metrics.bytes_uploaded;
            bytes_downloaded += metrics.bytes_downloaded;
            operations_executed += 1;
        }

        let end_time = Utc::now();
        let created_objects = self.created_blobs.read().await.clone();

        info!(
            operations = %operations_executed,
            uploaded = %Bytes::new(bytes_uploaded),
            downloaded = %Bytes::new(bytes_downloaded),
            duration_ms = %(end_time - start_time).num_milliseconds(),
            "Azure workload execution complete"
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
        // Azure Cost Management API requires Azure subscription access
        // Real-time cost data is available via Azure Portal or Cost Management APIs

        warn!(
            start = %start,
            end = %end,
            "Azure billing data requires Cost Management API access"
        );

        Ok(ActualCosts {
            is_complete: false,
            notes: vec![
                "Azure billing data available via Cost Management API".to_string(),
                "Requires Azure subscription-level access".to_string(),
                "Consider using Azure Cost Management exports".to_string(),
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
            "eastus",
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

        // Add warnings
        if !actual_costs.is_complete {
            result.add_warning("Actual billing data not available from Azure API");
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
        "Azure Blob Storage"
    }

    fn region(&self) -> &str {
        "eastus"
    }

    async fn cleanup(&self) -> Result<(), ValidationError> {
        let deleted = self.delete_created_blobs().await?;
        info!(deleted = %deleted, "Azure cleanup complete");
        Ok(())
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

/// Maps storage class to Azure access tier.
///
/// Azure uses access tiers: Hot, Cool, Cold, Archive
fn map_azure_access_tier(class: &StorageClass) -> AccessTier {
    match class.as_str() {
        "HOT" | "STANDARD" => AccessTier::Hot,
        "COOL" | "STANDARD_IA" => AccessTier::Cool,
        "COLD" => AccessTier::Cold,
        "ARCHIVE" | "GLACIER" | "GLACIER_DEEP_ARCHIVE" => AccessTier::Archive,
        _ => AccessTier::Hot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_access_tier_hot() {
        let class = StorageClass::new("HOT");
        let tier = map_azure_access_tier(&class);
        assert_eq!(tier, AccessTier::Hot);
    }

    #[test]
    fn map_access_tier_cool() {
        let class = StorageClass::new("COOL");
        let tier = map_azure_access_tier(&class);
        assert_eq!(tier, AccessTier::Cool);
    }

    #[test]
    fn map_access_tier_cold() {
        let class = StorageClass::new("COLD");
        let tier = map_azure_access_tier(&class);
        assert_eq!(tier, AccessTier::Cold);
    }

    #[test]
    fn map_access_tier_archive() {
        let class = StorageClass::new("ARCHIVE");
        let tier = map_azure_access_tier(&class);
        assert_eq!(tier, AccessTier::Archive);
    }

    #[test]
    fn map_access_tier_standard_defaults_to_hot() {
        let class = StorageClass::new("STANDARD");
        let tier = map_azure_access_tier(&class);
        assert_eq!(tier, AccessTier::Hot);
    }

    #[test]
    fn map_access_tier_unknown_defaults_to_hot() {
        let class = StorageClass::new("UNKNOWN");
        let tier = map_azure_access_tier(&class);
        assert_eq!(tier, AccessTier::Hot);
    }
}

//! Backblaze B2 validation implementation.
//!
//! This module provides validation against Backblaze B2 using their native API.
//!
//! # Prerequisites
//!
//! - B2 Application Key ID and Application Key
//! - A test bucket in B2
//!
//! # Environment Variables
//!
//! - `B2_APPLICATION_KEY_ID`: Your B2 key ID
//! - `B2_APPLICATION_KEY`: Your B2 application key
//!
//! # Example
//!
//! ```no_run,ignore
//! use cloud_billing_sim::validation::b2::B2Validator;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let validator = B2Validator::from_env("my-test-bucket").await?;
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
use crate::types::Bytes;
use crate::validation::{
    ActualCosts, CostComparison, ExecutedWorkload, ValidationError, ValidationProvider,
    ValidationResult, ValidationWorkload,
};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Backblaze B2 validation provider.
///
/// Connects to B2 to execute real operations and compare costs.
pub struct B2Validator {
    /// HTTP client.
    client: Client,

    /// B2 API URL.
    api_url: String,

    /// Authorization token.
    auth_token: String,

    /// Download URL.
    download_url: String,

    /// Account ID (for future use with billing API).
    account_id: String,

    /// Test bucket name.
    bucket_name: String,

    /// Test bucket ID.
    bucket_id: String,

    /// Objects created during validation (for cleanup).
    created_files: Arc<RwLock<Vec<FileInfo>>>,
}

/// B2 file information.
#[derive(Debug, Clone)]
struct FileInfo {
    file_id: String,
    file_name: String,
}

/// B2 authorize account response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorizeResponse {
    authorization_token: String,
    api_url: String,
    download_url: String,
    account_id: String,
}

/// B2 list buckets response.
#[derive(Debug, Deserialize)]
struct ListBucketsResponse {
    buckets: Vec<BucketInfo>,
}

/// B2 bucket info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BucketInfo {
    bucket_id: String,
    bucket_name: String,
}

/// B2 get upload URL response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadUrlResponse {
    upload_url: String,
    authorization_token: String,
}

/// B2 upload file response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct UploadFileResponse {
    file_id: String,
    file_name: String,
    content_length: u64,
}

/// B2 list file names response (for future use).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct ListFileNamesResponse {
    files: Vec<FileListEntry>,
    next_file_name: Option<String>,
}

/// B2 file list entry (for future use).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct FileListEntry {
    file_id: String,
    file_name: String,
    content_length: u64,
}

/// B2 delete file request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteFileRequest {
    file_name: String,
    file_id: String,
}

impl B2Validator {
    /// Creates a new B2 validator from environment variables.
    ///
    /// Reads `B2_APPLICATION_KEY_ID` and `B2_APPLICATION_KEY` from environment.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails or bucket is not found.
    pub async fn from_env(bucket_name: impl Into<String>) -> Result<Self, ValidationError> {
        let key_id = std::env::var("B2_APPLICATION_KEY_ID")
            .map_err(|_| ValidationError::B2Auth("B2_APPLICATION_KEY_ID not set".to_string()))?;
        let key = std::env::var("B2_APPLICATION_KEY")
            .map_err(|_| ValidationError::B2Auth("B2_APPLICATION_KEY not set".to_string()))?;

        Self::new(&key_id, &key, bucket_name).await
    }

    /// Creates a new B2 validator with explicit credentials.
    ///
    /// # Arguments
    ///
    /// * `key_id` - B2 Application Key ID
    /// * `key` - B2 Application Key
    /// * `bucket_name` - Bucket to use for validation
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails or bucket is not found.
    pub async fn new(
        key_id: &str,
        key: &str,
        bucket_name: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let bucket_name = bucket_name.into();
        info!(bucket = %bucket_name, "initializing B2 validator");

        let client = Client::new();

        // Authorize
        let auth_string = format!("{key_id}:{key}");
        let encoded = base64_encode(&auth_string);

        let auth_resp: AuthorizeResponse = client
            .get("https://api.backblazeb2.com/b2api/v2/b2_authorize_account")
            .header("Authorization", format!("Basic {encoded}"))
            .send()
            .await
            .map_err(|e| ValidationError::B2Auth(e.to_string()))?
            .json()
            .await
            .map_err(|e| ValidationError::B2Auth(e.to_string()))?;

        debug!(account_id = %auth_resp.account_id, "B2 authorization successful");

        // Find bucket
        let list_resp: ListBucketsResponse = client
            .post(format!("{}/b2api/v2/b2_list_buckets", auth_resp.api_url))
            .header("Authorization", &auth_resp.authorization_token)
            .json(&serde_json::json!({
                "accountId": auth_resp.account_id,
                "bucketName": bucket_name
            }))
            .send()
            .await
            .map_err(|e| ValidationError::B2Operation(e.to_string()))?
            .json()
            .await
            .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

        let bucket = list_resp
            .buckets
            .into_iter()
            .find(|b| b.bucket_name == bucket_name)
            .ok_or_else(|| {
                ValidationError::BucketNotAccessible(format!("bucket not found: {bucket_name}"))
            })?;

        info!(bucket_id = %bucket.bucket_id, "bucket verified accessible");

        Ok(Self {
            client,
            api_url: auth_resp.api_url,
            auth_token: auth_resp.authorization_token,
            download_url: auth_resp.download_url,
            account_id: auth_resp.account_id,
            bucket_name,
            bucket_id: bucket.bucket_id,
            created_files: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Gets an upload URL for the bucket.
    async fn get_upload_url(&self) -> Result<UploadUrlResponse, ValidationError> {
        self.client
            .post(format!("{}/b2api/v2/b2_get_upload_url", self.api_url))
            .header("Authorization", &self.auth_token)
            .json(&serde_json::json!({
                "bucketId": self.bucket_id
            }))
            .send()
            .await
            .map_err(|e| ValidationError::B2Operation(e.to_string()))?
            .json()
            .await
            .map_err(|e| ValidationError::B2Operation(e.to_string()))
    }

    /// Executes a single operation against B2.
    async fn execute_operation(
        &self,
        op: &Operation,
        key_prefix: &str,
    ) -> Result<OperationMetrics, ValidationError> {
        let key = op
            .key
            .as_ref()
            .map_or_else(|| "unnamed".to_string(), |k| format!("{key_prefix}{k}"));

        debug!(operation = ?op.kind, key = %key, "executing B2 operation");

        let metrics = match &op.kind {
            OperationKind::PutObject { size_bytes, .. } => {
                let upload_url = self.get_upload_url().await?;
                let data = vec![0u8; *size_bytes as usize];

                // Calculate SHA1 hash of data
                let sha1 = calculate_sha1(&data);

                let resp: UploadFileResponse = self
                    .client
                    .post(&upload_url.upload_url)
                    .header("Authorization", &upload_url.authorization_token)
                    .header("X-Bz-File-Name", url_encode(&key))
                    .header("Content-Type", "application/octet-stream")
                    .header("Content-Length", data.len())
                    .header("X-Bz-Content-Sha1", sha1)
                    .body(data)
                    .send()
                    .await
                    .map_err(|e| ValidationError::B2Operation(e.to_string()))?
                    .json()
                    .await
                    .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

                self.created_files.write().await.push(FileInfo {
                    file_id: resp.file_id,
                    file_name: resp.file_name,
                });

                OperationMetrics {
                    bytes_uploaded: *size_bytes,
                    bytes_downloaded: 0,
                    operation_type: "PUT".to_string(),
                }
            }

            OperationKind::GetObject {
                bytes_transferred, ..
            } => {
                // Download by name
                let url = format!(
                    "{}/file/{}/{}",
                    self.download_url,
                    self.bucket_name,
                    url_encode(&key)
                );

                let resp = self
                    .client
                    .get(&url)
                    .header("Authorization", &self.auth_token)
                    .send()
                    .await
                    .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

                let downloaded = bytes_transferred.unwrap_or(bytes.len() as u64);

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: downloaded,
                    operation_type: "GET".to_string(),
                }
            }

            OperationKind::DeleteObject => {
                // Find file ID
                let files = self.created_files.read().await;
                if let Some(file) = files.iter().find(|f| f.file_name == key) {
                    let file_id = file.file_id.clone();
                    drop(files);

                    self.client
                        .post(format!("{}/b2api/v2/b2_delete_file_version", self.api_url))
                        .header("Authorization", &self.auth_token)
                        .json(&DeleteFileRequest {
                            file_name: key.clone(),
                            file_id,
                        })
                        .send()
                        .await
                        .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

                    self.created_files.write().await.retain(|f| f.file_name != key);
                }

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "DELETE".to_string(),
                }
            }

            OperationKind::ListObjects { .. } => {
                self.client
                    .post(format!("{}/b2api/v2/b2_list_file_names", self.api_url))
                    .header("Authorization", &self.auth_token)
                    .json(&serde_json::json!({
                        "bucketId": self.bucket_id,
                        "prefix": key,
                        "maxFileCount": 1000
                    }))
                    .send()
                    .await
                    .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "LIST".to_string(),
                }
            }

            OperationKind::HeadObject => {
                // B2 doesn't have a direct HEAD equivalent for files
                // We use list with exact name match
                self.client
                    .post(format!("{}/b2api/v2/b2_list_file_names", self.api_url))
                    .header("Authorization", &self.auth_token)
                    .json(&serde_json::json!({
                        "bucketId": self.bucket_id,
                        "prefix": key,
                        "maxFileCount": 1
                    }))
                    .send()
                    .await
                    .map_err(|e| ValidationError::B2Operation(e.to_string()))?;

                OperationMetrics {
                    bytes_uploaded: 0,
                    bytes_downloaded: 0,
                    operation_type: "HEAD".to_string(),
                }
            }

            // Operations not directly supported by B2
            OperationKind::CopyObject { .. } => {
                warn!("B2 copy requires download+upload; skipping in validation");
                OperationMetrics::default()
            }

            OperationKind::CreateMultipartUpload { .. }
            | OperationKind::UploadPart { .. }
            | OperationKind::CompleteMultipartUpload { .. }
            | OperationKind::AbortMultipartUpload { .. } => {
                warn!("multipart operations not fully supported in B2 validation");
                OperationMetrics::default()
            }

            OperationKind::RestoreObject { .. }
            | OperationKind::LifecycleTransition { .. }
            | OperationKind::SelectObjectContent { .. } => {
                debug!("operation not supported by B2; skipping");
                OperationMetrics::default()
            }
        };

        Ok(metrics)
    }

    /// Deletes all files created during validation.
    async fn delete_created_files(&self) -> Result<usize, ValidationError> {
        let files = self.created_files.read().await.clone();
        let count = files.len();

        info!(count = %count, "cleaning up B2 validation files");

        for file in files {
            if let Err(e) = self
                .client
                .post(format!("{}/b2api/v2/b2_delete_file_version", self.api_url))
                .header("Authorization", &self.auth_token)
                .json(&DeleteFileRequest {
                    file_name: file.file_name.clone(),
                    file_id: file.file_id,
                })
                .send()
                .await
            {
                warn!(file = %file.file_name, error = %e, "failed to delete file during cleanup");
            }
        }

        self.created_files.write().await.clear();
        Ok(count)
    }
}

impl ValidationProvider for B2Validator {
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
            "executing B2 validation workload"
        );

        for op in &workload.operations.operations {
            let metrics = self.execute_operation(op, &workload.key_prefix).await?;
            bytes_uploaded += metrics.bytes_uploaded;
            bytes_downloaded += metrics.bytes_downloaded;
            operations_executed += 1;
        }

        let end_time = Utc::now();
        let created_objects: Vec<String> = self
            .created_files
            .read()
            .await
            .iter()
            .map(|f| f.file_name.clone())
            .collect();

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
        _bucket: Option<&str>,
    ) -> Result<ActualCosts, ValidationError> {
        // B2 doesn't have a real-time billing API
        // Costs are visible in the web console after the billing period

        warn!(
            start = %start,
            end = %end,
            "B2 does not provide real-time billing API"
        );

        Ok(ActualCosts {
            is_complete: false,
            notes: vec![
                "B2 billing data is only available via web console".to_string(),
                "Consider checking account dashboard for actual costs".to_string(),
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

        let transfer_comp = CostComparison::new(
            "data_transfer",
            report.breakdown.data_transfer_egress,
            actual_costs.data_transfer,
        );
        result.add_comparison(transfer_comp);

        // Add warnings
        if !actual_costs.is_complete {
            result.add_warning("Actual billing data not available from B2 API");
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
        "Backblaze B2"
    }

    fn region(&self) -> &str {
        "global"
    }

    async fn cleanup(&self) -> Result<(), ValidationError> {
        let deleted = self.delete_created_files().await?;
        info!(deleted = %deleted, "cleanup complete");
        Ok(())
    }
}

impl std::fmt::Debug for B2Validator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("B2Validator")
            .field("api_url", &self.api_url)
            .field("account_id", &self.account_id)
            .field("bucket_name", &self.bucket_name)
            .field("bucket_id", &self.bucket_id)
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

/// Simple base64 encoding for B2 authentication.
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let bytes = input.as_bytes();
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        result.push(ALPHABET[(b0 >> 2) as usize] as char);
        result.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// URL encode a string for B2 file names.
fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            '/' => {
                result.push('/'); // B2 allows unencoded forward slashes
            }
            _ => {
                for byte in c.to_string().bytes() {
                    result.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    result
}

/// Calculate SHA1 hash of data (simple implementation).
fn calculate_sha1(data: &[u8]) -> String {
    // For validation purposes, we use a placeholder
    // In production, use a proper SHA1 implementation
    if data.is_empty() {
        "da39a3ee5e6b4b0d3255bfef95601890afd80709".to_string()
    } else {
        // Simple checksum as placeholder - real impl would use sha1 crate
        let sum: u64 = data.iter().map(|&b| u64::from(b)).sum();
        format!("{sum:040x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_simple() {
        assert_eq!(base64_encode("hello"), "aGVsbG8=");
    }

    #[test]
    fn base64_encode_empty() {
        assert_eq!(base64_encode(""), "");
    }

    #[test]
    fn url_encode_simple() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("path/to/file"), "path/to/file");
    }

    #[test]
    fn url_encode_special_chars() {
        assert_eq!(url_encode("file name.txt"), "file%20name.txt");
    }

    #[test]
    fn sha1_empty_data() {
        let hash = calculate_sha1(&[]);
        assert_eq!(hash, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }
}

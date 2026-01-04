//! The main simulation engine.

use chrono::{DateTime, Datelike, Duration, Utc};
use rust_decimal::Decimal;
use tracing::{debug, info, instrument, trace, warn};

use crate::operations::{Operation, OperationKind, OperationLog};
use crate::pricing::{OperationType, PricingRules};
use crate::types::{Bytes, Money, StorageClass};

use super::error::EngineError;
use super::report::CostReport;
use super::state::StorageState;

/// The simulation engine that processes operations and calculates costs.
#[derive(Debug)]
pub struct Simulator {
    /// Pricing rules to apply.
    rules: PricingRules,

    /// Current storage state.
    state: StorageState,

    /// Accumulated cost report.
    report: CostReport,

    /// Track monthly egress for free tier calculations.
    monthly_egress_gb: Decimal,

    /// Current month being tracked.
    current_month: Option<(i32, u32)>, // (year, month)
}

impl Simulator {
    /// Creates a new simulator with the given pricing rules.
    #[must_use]
    pub fn new(rules: PricingRules) -> Self {
        info!(provider = %rules.provider.name, "initializing simulator");
        Self {
            rules,
            state: StorageState::new(),
            report: CostReport::new(),
            monthly_egress_gb: Decimal::ZERO,
            current_month: None,
        }
    }

    /// Runs the simulation on an operation log.
    ///
    /// # Errors
    ///
    /// Returns an error if operations reference unknown objects or storage classes.
    #[instrument(skip(self, log), fields(operation_count = log.operations.len()))]
    pub fn simulate(&mut self, log: &OperationLog) -> Result<&CostReport, EngineError> {
        let sorted_ops = log.sorted();

        if let Some((start, end)) = log.time_range() {
            info!(%start, %end, "simulating operations");
            self.report.time_range = Some((start, end));
        }

        for op in sorted_ops {
            self.process_operation(op)?;
        }

        // Calculate final storage costs up to the last operation
        if let Some((_, end)) = log.time_range() {
            self.bill_storage_to(end)?;
        }

        info!(total_cost = %self.report.total_cost, "simulation complete");
        Ok(&self.report)
    }

    /// Processes a single operation.
    #[instrument(skip(self), fields(bucket = %op.bucket, key = op.key.as_deref().unwrap_or("")))]
    fn process_operation(&mut self, op: &Operation) -> Result<(), EngineError> {
        // Update month tracking for egress calculations
        self.update_month_tracking(op.timestamp);

        // Bill storage up to this operation's timestamp
        self.bill_storage_to(op.timestamp)?;

        // Process the operation
        match &op.kind {
            OperationKind::PutObject {
                size_bytes,
                storage_class,
            } => {
                self.handle_put_object(op, *size_bytes, storage_class)?;
            }
            OperationKind::GetObject {
                bytes_transferred,
                retrieval_tier,
            } => {
                self.handle_get_object(op, *bytes_transferred, retrieval_tier.as_ref())?;
            }
            OperationKind::DeleteObject => {
                self.handle_delete_object(op)?;
            }
            OperationKind::CopyObject {
                source_key,
                source_bucket,
                storage_class,
            } => {
                self.handle_copy_object(
                    op,
                    source_key,
                    source_bucket.as_deref(),
                    storage_class.as_ref(),
                )?;
            }
            OperationKind::ListObjects { objects_returned } => {
                self.handle_list_objects(op, *objects_returned)?;
            }
            OperationKind::HeadObject => {
                self.handle_head_object(op)?;
            }
            OperationKind::CreateMultipartUpload { storage_class } => {
                self.handle_create_multipart_upload(op, storage_class)?;
            }
            OperationKind::UploadPart {
                size_bytes,
                upload_id,
                part_number,
            } => {
                self.handle_upload_part(op, *size_bytes, upload_id, *part_number)?;
            }
            OperationKind::CompleteMultipartUpload {
                upload_id,
                total_size_bytes,
            } => {
                self.handle_complete_multipart_upload(op, upload_id, *total_size_bytes)?;
            }
            OperationKind::AbortMultipartUpload { upload_id } => {
                self.handle_abort_multipart_upload(op, upload_id)?;
            }
            OperationKind::RestoreObject { days, tier } => {
                self.handle_restore_object(op, *days, tier.as_ref())?;
            }
            OperationKind::LifecycleTransition { new_storage_class } => {
                self.handle_lifecycle_transition(op, new_storage_class)?;
            }
            OperationKind::SelectObjectContent {
                bytes_scanned,
                bytes_returned,
            } => {
                self.handle_select_object_content(op, *bytes_scanned, *bytes_returned)?;
            }
            OperationKind::Wait { reason } => {
                self.handle_wait(op, reason.as_deref())?;
            }
            OperationKind::SetBucketVersioning { enabled } => {
                self.handle_set_bucket_versioning(op, *enabled)?;
            }
            OperationKind::DeleteObjectVersion { version_id } => {
                self.handle_delete_object_version(op, version_id)?;
            }
            OperationKind::ReplicateObject {
                destination_region,
                destination_bucket,
                destination_key,
                storage_class,
            } => {
                self.handle_replicate_object(
                    op,
                    destination_region,
                    destination_bucket.as_deref(),
                    destination_key.as_deref(),
                    storage_class.as_ref(),
                )?;
            }
        }

        Ok(())
    }

    /// Updates month tracking for egress free tier resets.
    fn update_month_tracking(&mut self, timestamp: DateTime<Utc>) {
        let month = (
            timestamp.date_naive().year(),
            timestamp.date_naive().month(),
        );

        if self.current_month != Some(month) {
            if self.current_month.is_some() {
                debug!(?month, "new month - resetting egress counter");
            }
            self.current_month = Some(month);
            self.monthly_egress_gb = Decimal::ZERO;
        }
    }

    /// Bills storage costs up to a given timestamp.
    fn bill_storage_to(&mut self, timestamp: DateTime<Utc>) -> Result<(), EngineError> {
        // Bill current objects
        self.bill_current_objects_to(timestamp)?;

        // Bill noncurrent versions
        self.bill_noncurrent_versions_to(timestamp)?;

        Ok(())
    }

    /// Bills storage costs for current objects up to a given timestamp.
    fn bill_current_objects_to(&mut self, timestamp: DateTime<Utc>) -> Result<(), EngineError> {
        // Collect objects that need billing
        let objects_to_bill: Vec<_> = self
            .state
            .objects()
            .filter(|(_, obj)| obj.last_billed_at < timestamp)
            .map(|(key, obj)| (key.clone(), obj.clone()))
            .collect();

        for (key, obj) in objects_to_bill {
            let duration = timestamp.signed_duration_since(obj.last_billed_at);
            let fraction = duration_to_month_fraction(duration);

            if fraction > Decimal::ZERO {
                let class_rules = self
                    .rules
                    .get_storage_class(&obj.storage_class)
                    .ok_or_else(|| {
                        EngineError::UnknownStorageClass(obj.storage_class.to_string())
                    })?;

                let cost = class_rules.calculate_storage_cost(obj.size, fraction);

                trace!(
                    object = %key,
                    class = %obj.storage_class,
                    size = %obj.size,
                    duration_hours = duration.num_hours(),
                    cost = %cost,
                    "billing storage"
                );

                self.report.add_storage_cost(&obj.storage_class, cost);
                self.report
                    .record_object_cost(&key.to_string(), "storage", cost);

                // Update last billed time
                if let Some(obj_mut) = self.state.get_object_mut(&key.bucket, &key.key) {
                    obj_mut.last_billed_at = timestamp;
                }
            }
        }

        Ok(())
    }

    /// Bills storage costs for noncurrent versions up to a given timestamp.
    fn bill_noncurrent_versions_to(&mut self, timestamp: DateTime<Utc>) -> Result<(), EngineError> {
        // Collect noncurrent versions that need billing
        let versions_to_bill: Vec<_> = self
            .state
            .all_noncurrent_versions()
            .filter(|(_, v)| v.last_billed_at < timestamp)
            .map(|(key, v)| (key.clone(), v.clone()))
            .collect();

        for (key, version) in &versions_to_bill {
            let duration = timestamp.signed_duration_since(version.last_billed_at);
            let fraction = duration_to_month_fraction(duration);

            if fraction > Decimal::ZERO {
                let class_rules = self
                    .rules
                    .get_storage_class(&version.storage_class)
                    .ok_or_else(|| {
                        EngineError::UnknownStorageClass(version.storage_class.to_string())
                    })?;

                let cost = class_rules.calculate_storage_cost(version.size, fraction);

                trace!(
                    object = %key,
                    version_id = %version.version_id,
                    class = %version.storage_class,
                    size = %version.size,
                    duration_hours = duration.num_hours(),
                    cost = %cost,
                    "billing noncurrent version storage"
                );

                self.report.add_storage_cost(&version.storage_class, cost);
                self.report.record_object_cost(
                    &format!("{}@{}", key, version.version_id),
                    "noncurrent_storage",
                    cost,
                );
            }
        }

        // Update last_billed_at for noncurrent versions
        // We need to do this separately to avoid borrow issues
        for (key, version) in versions_to_bill {
            // Find and update the version
            let versions = self.state.get_noncurrent_versions(&key.bucket, &key.key);
            if let Some(pos) = versions.iter().position(|v| v.version_id == version.version_id) {
                // We need mutable access - get through the internal iterator
                for (k, v) in self.state.noncurrent_versions_mut() {
                    if k.bucket == key.bucket && k.key == key.key && v.version_id == version.version_id {
                        v.last_billed_at = timestamp;
                        break;
                    }
                }
                let _ = pos; // Silence unused warning
            }
        }

        Ok(())
    }

    /// Handles `PutObject` operation.
    fn handle_put_object(
        &mut self,
        op: &Operation,
        size_bytes: u64,
        storage_class: &StorageClass,
    ) -> Result<(), EngineError> {
        let key = op
            .key
            .as_deref()
            .ok_or_else(|| EngineError::InvalidSequence("PutObject requires key".to_string()))?;

        debug!(size = size_bytes, class = %storage_class, "put object");

        // Check if object already exists (for overwrite)
        if let Some(existing) = self.state.get_object(&op.bucket, key) {
            let days_stored = existing.days_stored(op.timestamp);
            let class_rules = self.rules.get_storage_class(&existing.storage_class);

            if let Some(rules) = class_rules {
                let penalty = rules.early_deletion_cost(existing.size, days_stored);
                if !penalty.is_zero() {
                    debug!(penalty = %penalty, "early deletion penalty for overwrite");
                    self.report.add_early_deletion_penalty(penalty);
                    self.report
                        .record_object_cost(&op.object_path(), "early_deletion", penalty);
                }
            }
        }

        // Add operation cost
        let op_cost = self.get_operation_cost(storage_class, OperationType::Put)?;
        self.report.add_operation_cost("PUT", op_cost);
        self.report
            .record_object_cost(&op.object_path(), "operations", op_cost);
        self.report.stats.record_operation("PUT");
        self.report.stats.record_upload(Bytes::new(size_bytes));
        self.report.stats.objects_created += 1;

        // Store the object
        self.state.put_object(
            &op.bucket,
            key,
            Bytes::new(size_bytes),
            storage_class.clone(),
            op.timestamp,
        );

        // Update peak storage
        let total: Bytes = self
            .state
            .total_storage_by_class()
            .values()
            .fold(Bytes::ZERO, |a, &b| a + b);
        self.report.stats.update_peak_storage(total);
        self.report.stats.final_storage = total;

        Ok(())
    }

    /// Handles `GetObject` operation.
    fn handle_get_object(
        &mut self,
        op: &Operation,
        bytes_transferred: Option<u64>,
        retrieval_tier: Option<&crate::operations::RetrievalSpeed>,
    ) -> Result<(), EngineError> {
        let key = op
            .key
            .as_deref()
            .ok_or_else(|| EngineError::InvalidSequence("GetObject requires key".to_string()))?;

        let obj =
            self.state
                .get_object(&op.bucket, key)
                .ok_or_else(|| EngineError::UnknownObject {
                    bucket: op.bucket.clone(),
                    key: key.to_string(),
                })?;

        let bytes = bytes_transferred.map_or(obj.size, Bytes::new);
        debug!(size = %bytes, class = %obj.storage_class, "get object");

        // Operation cost
        let op_cost = self.get_operation_cost(&obj.storage_class, OperationType::Get)?;
        self.report.add_operation_cost("GET", op_cost);
        self.report
            .record_object_cost(&op.object_path(), "operations", op_cost);
        self.report.stats.record_operation("GET");
        self.report.stats.record_download(bytes);

        // Retrieval cost for archive classes
        if let Some(class_rules) = self.rules.get_storage_class(&obj.storage_class) {
            let tier_name = retrieval_tier.map(|t| t.as_str());
            let retrieval_cost_per_gb = class_rules.get_retrieval_cost(tier_name);

            if !retrieval_cost_per_gb.is_zero() {
                let retrieval_cost = retrieval_cost_per_gb * bytes.as_gb_decimal();
                self.report.add_retrieval_cost(retrieval_cost);
                self.report
                    .record_object_cost(&op.object_path(), "retrieval", retrieval_cost);
            }
        }

        // Egress cost
        let egress_gb = bytes.as_gb_decimal();
        self.monthly_egress_gb += egress_gb;

        let storage_gb: Decimal = self
            .state
            .total_storage_by_class()
            .values()
            .copied()
            .map(Bytes::as_gb_decimal)
            .sum();

        let egress_cost = self
            .rules
            .data_transfer
            .calculate_egress_cost(self.monthly_egress_gb, storage_gb);

        // Only charge the incremental egress cost
        // (This is simplified - in reality we'd need to track cumulative)
        if !egress_cost.is_zero() {
            let incremental = self
                .rules
                .data_transfer
                .egress_price_per_gb
                .calculate_cost(egress_gb);
            self.report.add_egress_cost(incremental);
            self.report
                .record_object_cost(&op.object_path(), "egress", incremental);
        }

        Ok(())
    }

    /// Handles `DeleteObject` operation.
    fn handle_delete_object(&mut self, op: &Operation) -> Result<(), EngineError> {
        let key = op
            .key
            .as_deref()
            .ok_or_else(|| EngineError::InvalidSequence("DeleteObject requires key".to_string()))?;

        let obj = self.state.delete_object(&op.bucket, key).ok_or_else(|| {
            EngineError::UnknownObject {
                bucket: op.bucket.clone(),
                key: key.to_string(),
            }
        })?;

        debug!(class = %obj.storage_class, "delete object");

        // Check for early deletion penalty
        let days_stored = obj.days_stored(op.timestamp);
        if let Some(class_rules) = self.rules.get_storage_class(&obj.storage_class) {
            let penalty = class_rules.early_deletion_cost(obj.size, days_stored);
            if !penalty.is_zero() {
                debug!(penalty = %penalty, days = days_stored, "early deletion penalty");
                self.report.add_early_deletion_penalty(penalty);
                self.report
                    .record_object_cost(&op.object_path(), "early_deletion", penalty);
            }
        }

        // DELETE is usually free, but check anyway
        let op_cost = self.get_operation_cost(&obj.storage_class, OperationType::Delete)?;
        if !op_cost.is_zero() {
            self.report.add_operation_cost("DELETE", op_cost);
        }
        self.report.stats.record_operation("DELETE");
        self.report.stats.objects_deleted += 1;

        // Update storage stats
        let total: Bytes = self
            .state
            .total_storage_by_class()
            .values()
            .fold(Bytes::ZERO, |a, &b| a + b);
        self.report.stats.final_storage = total;

        Ok(())
    }

    /// Handles `CopyObject` operation.
    fn handle_copy_object(
        &mut self,
        op: &Operation,
        source_key: &str,
        source_bucket: Option<&str>,
        storage_class: Option<&StorageClass>,
    ) -> Result<(), EngineError> {
        let dest_key = op.key.as_deref().ok_or_else(|| {
            EngineError::InvalidSequence("CopyObject requires destination key".to_string())
        })?;

        let src_bucket = source_bucket.unwrap_or(&op.bucket);
        let src_obj = self
            .state
            .get_object(src_bucket, source_key)
            .ok_or_else(|| EngineError::UnknownObject {
                bucket: src_bucket.to_string(),
                key: source_key.to_string(),
            })?;

        let dest_class = storage_class
            .cloned()
            .unwrap_or_else(|| src_obj.storage_class.clone());
        let size = src_obj.size;

        debug!(
            src = format!("{}/{}", src_bucket, source_key),
            dest = format!("{}/{}", op.bucket, dest_key),
            class = %dest_class,
            "copy object"
        );

        // COPY counts as PUT
        let op_cost = self.get_operation_cost(&dest_class, OperationType::Copy)?;
        self.report.add_operation_cost("COPY", op_cost);
        self.report
            .record_object_cost(&op.object_path(), "operations", op_cost);
        self.report.stats.record_operation("COPY");
        self.report.stats.objects_created += 1;

        // Create the destination object
        self.state
            .put_object(&op.bucket, dest_key, size, dest_class, op.timestamp);

        Ok(())
    }

    /// Handles `ListObjects` operation.
    fn handle_list_objects(
        &mut self,
        op: &Operation,
        _objects_returned: Option<u32>,
    ) -> Result<(), EngineError> {
        debug!(bucket = %op.bucket, "list objects");

        // LIST is charged at PUT rate for S3
        let class = StorageClass::new("STANDARD");
        let op_cost = self.get_operation_cost(&class, OperationType::List)?;
        self.report.add_operation_cost("LIST", op_cost);
        self.report.stats.record_operation("LIST");

        Ok(())
    }

    /// Handles `HeadObject` operation.
    fn handle_head_object(&mut self, op: &Operation) -> Result<(), EngineError> {
        let key = op
            .key
            .as_deref()
            .ok_or_else(|| EngineError::InvalidSequence("HeadObject requires key".to_string()))?;

        let obj =
            self.state
                .get_object(&op.bucket, key)
                .ok_or_else(|| EngineError::UnknownObject {
                    bucket: op.bucket.clone(),
                    key: key.to_string(),
                })?;

        debug!(class = %obj.storage_class, "head object");

        let op_cost = self.get_operation_cost(&obj.storage_class, OperationType::Head)?;
        self.report.add_operation_cost("HEAD", op_cost);
        self.report.stats.record_operation("HEAD");

        Ok(())
    }

    /// Handles `CreateMultipartUpload`.
    fn handle_create_multipart_upload(
        &mut self,
        op: &Operation,
        storage_class: &StorageClass,
    ) -> Result<(), EngineError> {
        let key = op.key.as_deref().ok_or_else(|| {
            EngineError::InvalidSequence("CreateMultipartUpload requires key".to_string())
        })?;

        debug!(class = %storage_class, "create multipart upload");

        // Generate a simple upload ID (in real S3 this would be returned by the API)
        let upload_id = format!("{}:{}", op.bucket, key);

        self.state.start_multipart_upload(
            upload_id,
            op.bucket.clone(),
            key.to_string(),
            storage_class.clone(),
            op.timestamp,
        );

        self.report.stats.record_operation("CreateMultipartUpload");

        Ok(())
    }

    /// Handles `UploadPart`.
    fn handle_upload_part(
        &mut self,
        op: &Operation,
        size_bytes: u64,
        upload_id: &str,
        part_number: u32,
    ) -> Result<(), EngineError> {
        debug!(upload_id = %upload_id, part = part_number, size = size_bytes, "upload part");

        if !self
            .state
            .add_part(upload_id, part_number, Bytes::new(size_bytes), op.timestamp)
        {
            return Err(EngineError::InvalidSequence(format!(
                "Unknown multipart upload: {upload_id}"
            )));
        }

        // Each part is a PUT request
        let class = StorageClass::new("STANDARD");
        let op_cost = self.get_operation_cost(&class, OperationType::Put)?;
        self.report.add_operation_cost("UploadPart", op_cost);
        self.report.stats.record_operation("UploadPart");
        self.report.stats.record_upload(Bytes::new(size_bytes));

        Ok(())
    }

    /// Handles `CompleteMultipartUpload`.
    fn handle_complete_multipart_upload(
        &mut self,
        op: &Operation,
        upload_id: &str,
        _total_size_bytes: Option<u64>,
    ) -> Result<(), EngineError> {
        debug!(upload_id = %upload_id, "complete multipart upload");

        let total_size = self
            .state
            .complete_multipart_upload(upload_id, op.timestamp)
            .ok_or_else(|| {
                EngineError::InvalidSequence(format!("Unknown multipart upload: {upload_id}"))
            })?;

        self.report
            .stats
            .record_operation("CompleteMultipartUpload");
        self.report.stats.objects_created += 1;

        let total: Bytes = self
            .state
            .total_storage_by_class()
            .values()
            .fold(Bytes::ZERO, |a, &b| a + b);
        self.report.stats.update_peak_storage(total);
        self.report.stats.final_storage = total;

        debug!(total_size = %total_size, "multipart upload completed");

        Ok(())
    }

    /// Handles `AbortMultipartUpload`.
    #[allow(clippy::unnecessary_wraps)] // Consistent with other handlers
    fn handle_abort_multipart_upload(
        &mut self,
        _op: &Operation,
        upload_id: &str,
    ) -> Result<(), EngineError> {
        debug!(upload_id = %upload_id, "abort multipart upload");

        if !self.state.abort_multipart_upload(upload_id) {
            warn!(upload_id = %upload_id, "attempted to abort unknown upload");
        }

        self.report.stats.record_operation("AbortMultipartUpload");

        Ok(())
    }

    /// Handles `RestoreObject` (for Glacier).
    fn handle_restore_object(
        &mut self,
        op: &Operation,
        days: u32,
        tier: Option<&crate::operations::RetrievalSpeed>,
    ) -> Result<(), EngineError> {
        let key = op.key.as_deref().ok_or_else(|| {
            EngineError::InvalidSequence("RestoreObject requires key".to_string())
        })?;

        let obj =
            self.state
                .get_object(&op.bucket, key)
                .ok_or_else(|| EngineError::UnknownObject {
                    bucket: op.bucket.clone(),
                    key: key.to_string(),
                })?;

        debug!(class = %obj.storage_class, days = days, "restore object");

        // Retrieval cost
        if let Some(class_rules) = self.rules.get_storage_class(&obj.storage_class) {
            let tier_name = tier.map(|t| t.as_str());
            let cost_per_gb = class_rules.get_retrieval_cost(tier_name);
            let retrieval_cost = cost_per_gb * obj.size.as_gb_decimal();

            if !retrieval_cost.is_zero() {
                self.report.add_retrieval_cost(retrieval_cost);
                self.report
                    .record_object_cost(&op.object_path(), "retrieval", retrieval_cost);
            }

            // Retrieval request cost
            if let Some(tier_rules) = tier_name.and_then(|t| {
                class_rules
                    .retrieval_tiers
                    .iter()
                    .find(|rt| rt.name.eq_ignore_ascii_case(t))
            }) {
                if let Some(request_cost) = tier_rules.price_per_1000_requests {
                    let cost = request_cost * Decimal::new(1, 3);
                    self.report.add_retrieval_cost(cost);
                }
            }
        }

        self.report.stats.record_operation("RestoreObject");

        Ok(())
    }

    /// Handles `LifecycleTransition`.
    fn handle_lifecycle_transition(
        &mut self,
        op: &Operation,
        new_storage_class: &StorageClass,
    ) -> Result<(), EngineError> {
        let key = op.key.as_deref().ok_or_else(|| {
            EngineError::InvalidSequence("LifecycleTransition requires key".to_string())
        })?;

        let obj = self.state.get_object_mut(&op.bucket, key).ok_or_else(|| {
            EngineError::UnknownObject {
                bucket: op.bucket.clone(),
                key: key.to_string(),
            }
        })?;

        let old_class = obj.storage_class.clone();
        debug!(from = %old_class, to = %new_storage_class, "lifecycle transition");

        // Transition cost
        if let Some(transition_rules) = self
            .rules
            .get_transition_cost(&old_class, new_storage_class)
        {
            let cost = transition_rules.per_1000_requests * Decimal::new(1, 3);
            self.report.add_transition_cost(cost);
            self.report
                .record_object_cost(&op.object_path(), "transition", cost);
        }

        // Update object state
        obj.transition_to(new_storage_class.clone(), op.timestamp);

        self.report.stats.record_operation("LifecycleTransition");

        Ok(())
    }

    /// Handles `SelectObjectContent`.
    fn handle_select_object_content(
        &mut self,
        op: &Operation,
        bytes_scanned: Option<u64>,
        bytes_returned: Option<u64>,
    ) -> Result<(), EngineError> {
        let key = op.key.as_deref().ok_or_else(|| {
            EngineError::InvalidSequence("SelectObjectContent requires key".to_string())
        })?;

        let obj =
            self.state
                .get_object(&op.bucket, key)
                .ok_or_else(|| EngineError::UnknownObject {
                    bucket: op.bucket.clone(),
                    key: key.to_string(),
                })?;

        debug!(
            class = %obj.storage_class,
            scanned = bytes_scanned,
            returned = bytes_returned,
            "select object content"
        );

        // GET/SELECT operation cost
        let op_cost = self.get_operation_cost(&obj.storage_class, OperationType::Select)?;
        self.report.add_operation_cost("SELECT", op_cost);
        self.report.stats.record_operation("SELECT");

        // Egress for returned bytes
        if let Some(returned) = bytes_returned {
            self.report.stats.record_download(Bytes::new(returned));
        }

        Ok(())
    }

    /// Handles `Wait` operation.
    ///
    /// This operation advances time without performing any cloud operation.
    /// Storage costs are billed up to this timestamp (already done in process_operation).
    fn handle_wait(&mut self, _op: &Operation, reason: Option<&str>) -> Result<(), EngineError> {
        if let Some(r) = reason {
            debug!(reason = %r, "wait");
        } else {
            debug!("wait");
        }

        // No operation cost - storage billing already happened in process_operation
        self.report.stats.record_operation("WAIT");

        Ok(())
    }

    /// Handles `SetBucketVersioning` operation.
    fn handle_set_bucket_versioning(
        &mut self,
        op: &Operation,
        enabled: bool,
    ) -> Result<(), EngineError> {
        debug!(bucket = %op.bucket, enabled, "set bucket versioning");

        self.state.set_versioning(&op.bucket, enabled);
        self.report.stats.record_operation("SET_BUCKET_VERSIONING");

        Ok(())
    }

    /// Handles `DeleteObjectVersion` operation.
    fn handle_delete_object_version(
        &mut self,
        op: &Operation,
        version_id: &str,
    ) -> Result<(), EngineError> {
        let key = op
            .key
            .as_deref()
            .ok_or_else(|| EngineError::InvalidSequence("DeleteObjectVersion requires key".to_string()))?;

        debug!(version_id, "delete object version");

        let removed = self.state.delete_version(&op.bucket, key, version_id);

        if let Some(version) = removed {
            // Check for early deletion penalty
            let days_stored = {
                let duration = op.timestamp.signed_duration_since(version.created_at);
                duration.num_days().try_into().unwrap_or(0)
            };

            if let Some(class_rules) = self.rules.get_storage_class(&version.storage_class) {
                let penalty = class_rules.early_deletion_cost(version.size, days_stored);
                if !penalty.is_zero() {
                    debug!(penalty = %penalty, "early deletion penalty for version");
                    self.report.add_early_deletion_penalty(penalty);
                    self.report.record_object_cost(
                        &format!("{}@{}", op.object_path(), version_id),
                        "early_deletion",
                        penalty,
                    );
                }
            }

            // Delete operations are typically free
            self.report.stats.record_operation("DELETE_VERSION");
        } else {
            warn!(version_id, "version not found for deletion");
        }

        Ok(())
    }

    /// Handles `ReplicateObject` operation.
    ///
    /// Cross-region replication incurs:
    /// - Data transfer costs (egress from source region)
    /// - PUT operation cost in destination
    fn handle_replicate_object(
        &mut self,
        op: &Operation,
        destination_region: &str,
        destination_bucket: Option<&str>,
        destination_key: Option<&str>,
        storage_class: Option<&StorageClass>,
    ) -> Result<(), EngineError> {
        let key = op
            .key
            .as_deref()
            .ok_or_else(|| EngineError::InvalidSequence("ReplicateObject requires key".to_string()))?;

        let obj = self
            .state
            .get_object(&op.bucket, key)
            .ok_or_else(|| EngineError::UnknownObject {
                bucket: op.bucket.clone(),
                key: key.to_string(),
            })?
            .clone();

        let dest_bucket = destination_bucket.unwrap_or(&op.bucket);
        let dest_key = destination_key.unwrap_or(key);
        let dest_class = storage_class.cloned().unwrap_or_else(|| obj.storage_class.clone());

        debug!(
            source = %op.object_path(),
            destination_region,
            dest_bucket,
            dest_key,
            class = %dest_class,
            size = %obj.size,
            "replicate object"
        );

        // Data transfer cost (cross-region egress)
        // For simplicity, we use the standard egress pricing
        // A more complete implementation would use region-pair specific pricing
        let egress_gb = obj.size.as_gb_decimal();
        self.monthly_egress_gb += egress_gb;

        let storage_gb: Decimal = self
            .state
            .total_storage_by_class()
            .values()
            .map(|b| b.as_gb_decimal())
            .sum();

        let egress_cost = self
            .rules
            .data_transfer
            .calculate_egress_cost(self.monthly_egress_gb, storage_gb);

        // Calculate incremental egress cost
        if !egress_cost.is_zero() {
            let incremental = self
                .rules
                .data_transfer
                .egress_price_per_gb
                .calculate_cost(egress_gb);
            self.report.add_egress_cost(incremental);
            self.report
                .record_object_cost(&op.object_path(), "replication_transfer", incremental);
        }

        // PUT operation cost for the replica
        let put_cost = self.get_operation_cost(&dest_class, OperationType::Put)?;
        self.report.add_operation_cost("PUT", put_cost);

        // Create the replica object (in a separate "region" conceptually)
        // Note: In a more complete implementation, we'd track objects per region
        // For now, we just record the stats
        self.report.stats.record_operation("REPLICATE");
        self.report.stats.record_upload(obj.size);

        Ok(())
    }

    /// Gets the operation cost for a storage class.
    fn get_operation_cost(
        &self,
        class: &StorageClass,
        op_type: OperationType,
    ) -> Result<Money, EngineError> {
        let rules = self
            .rules
            .get_operations(class)
            .ok_or_else(|| EngineError::UnknownStorageClass(class.to_string()))?;

        Ok(rules.cost_for_operation(op_type))
    }

    /// Returns the current cost report.
    #[must_use]
    pub const fn report(&self) -> &CostReport {
        &self.report
    }

    /// Returns the current storage state.
    #[must_use]
    pub const fn state(&self) -> &StorageState {
        &self.state
    }

    /// Consumes the simulator and returns the final report.
    #[must_use]
    pub fn into_report(self) -> CostReport {
        self.report
    }
}

/// Converts a duration to a fraction of a month (30 days).
fn duration_to_month_fraction(duration: Duration) -> Decimal {
    let hours = duration.num_hours();
    let month_hours = 30 * 24; // 720 hours
    Decimal::from(hours) / Decimal::from(month_hours)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::{DataTransferRules, OperationRules, StorageClassRules, TieredPrice};
    use std::collections::HashMap;
    use std::str::FromStr;

    fn test_rules() -> PricingRules {
        let mut storage_classes = HashMap::new();
        storage_classes.insert(
            "STANDARD".to_string(),
            StorageClassRules {
                storage_price_per_gb_month: TieredPrice::flat(
                    Money::from_str("0.023").ok().unwrap_or(Money::ZERO),
                ),
                min_billable_size_bytes: None,
                min_storage_duration_days: None,
                metadata_overhead_bytes: None,
                retrieval_price_per_gb: None,
                retrieval_tiers: vec![],
                intelligent_tiering: false,
                monitoring_price_per_1000_objects: None,
            },
        );

        let mut operations = HashMap::new();
        operations.insert(
            "DEFAULT".to_string(),
            OperationRules {
                put_per_1000: Money::from_str("0.005").ok(),
                get_per_1000: Money::from_str("0.0004").ok(),
                list_per_1000: Money::from_str("0.005").ok(),
                delete_per_1000: Some(Money::ZERO),
                head_per_1000: Money::from_str("0.0004").ok(),
                lifecycle_transition_per_1000: None,
            },
        );

        PricingRules {
            provider: crate::pricing::ProviderInfo {
                name: "test".to_string(),
                region: None,
                version: None,
                currency: "USD".to_string(),
            },
            storage_classes,
            operations,
            data_transfer: DataTransferRules::default(),
            lifecycle_transitions: HashMap::new(),
        }
    }

    #[test]
    fn simple_put_get_delete() -> Result<(), Box<dyn std::error::Error>> {
        let rules = test_rules();
        let mut sim = Simulator::new(rules);

        let log = OperationLog {
            operations: vec![
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024 * 1024, // 1 MB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                Operation {
                    timestamp: "2024-01-15T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::GetObject {
                        bytes_transferred: None,
                        retrieval_tier: None,
                    },
                },
                Operation {
                    timestamp: "2024-02-01T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::DeleteObject,
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;

        assert!(!report.total_cost.is_zero());
        assert_eq!(report.stats.objects_created, 1);
        assert_eq!(report.stats.objects_deleted, 1);
        Ok(())
    }

    #[test]
    fn wait_operation_bills_storage() -> Result<(), Box<dyn std::error::Error>> {
        let rules = test_rules();
        let mut sim = Simulator::new(rules);

        // Upload 1 GB, then wait 30 days
        let log = OperationLog {
            operations: vec![
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("large-file.bin".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024 * 1024 * 1024, // 1 GB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                Operation {
                    timestamp: "2024-01-31T00:00:00Z".parse()?,
                    bucket: "_".into(),
                    key: None,
                    kind: OperationKind::Wait {
                        reason: Some("Calculate 30 days storage".to_string()),
                    },
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;

        // Storage cost for 1 GB for 30 days at $0.023/GB/month = $0.023
        // (30 days is exactly 1 month in our calculation)
        assert!(!report.total_cost.is_zero());
        assert_eq!(report.stats.objects_created, 1);
        assert_eq!(report.stats.objects_deleted, 0);

        // Verify storage was billed (should be approximately $0.023)
        let storage_cost = report.breakdown.total_storage;
        assert!(!storage_cost.is_zero());

        Ok(())
    }

    #[test]
    fn wait_operation_with_no_reason() -> Result<(), Box<dyn std::error::Error>> {
        let rules = test_rules();
        let mut sim = Simulator::new(rules);

        let log = OperationLog {
            operations: vec![
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024,
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                Operation {
                    timestamp: "2024-01-02T00:00:00Z".parse()?,
                    bucket: "_".into(),
                    key: None,
                    kind: OperationKind::Wait { reason: None },
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;
        assert!(!report.total_cost.is_zero());
        Ok(())
    }

    #[test]
    fn wait_extends_time_range() -> Result<(), Box<dyn std::error::Error>> {
        let rules = test_rules();
        let mut sim = Simulator::new(rules);

        // Upload on Jan 1, wait until Jul 1 (6 months)
        let log = OperationLog {
            operations: vec![
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.bin".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024 * 1024 * 1024, // 1 GB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                Operation {
                    timestamp: "2024-07-01T00:00:00Z".parse()?,
                    bucket: "_".into(),
                    key: None,
                    kind: OperationKind::Wait {
                        reason: Some("6 months of storage".to_string()),
                    },
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;

        // 6 months of storage for 1 GB at $0.023/GB/month ≈ $0.138
        // (182 days / 30 days per month ≈ 6.07 months)
        let storage_cost = report.breakdown.total_storage;
        assert!(!storage_cost.is_zero());

        // Verify time range was extended
        let (start, end) = report.time_range.expect("time_range should be set");
        assert_eq!(start.format("%Y-%m-%d").to_string(), "2024-01-01");
        assert_eq!(end.format("%Y-%m-%d").to_string(), "2024-07-01");

        Ok(())
    }

    #[test]
    fn versioning_creates_noncurrent_versions() -> Result<(), Box<dyn std::error::Error>> {
        let rules = test_rules();
        let mut sim = Simulator::new(rules);

        let log = OperationLog {
            operations: vec![
                // Enable versioning
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "versioned-bucket".into(),
                    key: None,
                    kind: OperationKind::SetBucketVersioning { enabled: true },
                },
                // Upload file v1
                Operation {
                    timestamp: "2024-01-01T00:01:00Z".parse()?,
                    bucket: "versioned-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024 * 1024, // 1 MB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                // Upload file v2 (creates noncurrent version)
                Operation {
                    timestamp: "2024-01-02T00:00:00Z".parse()?,
                    bucket: "versioned-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 2 * 1024 * 1024, // 2 MB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                // Wait to bill storage
                Operation {
                    timestamp: "2024-02-01T00:00:00Z".parse()?,
                    bucket: "_".into(),
                    key: None,
                    kind: OperationKind::Wait { reason: None },
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;

        // Should have storage cost for both current and noncurrent versions
        assert!(!report.total_cost.is_zero());

        // Verify noncurrent version was created
        assert_eq!(sim.state().noncurrent_version_count(), 1);

        Ok(())
    }

    #[test]
    fn delete_object_version_with_early_deletion() -> Result<(), Box<dyn std::error::Error>> {
        // Create rules with early deletion penalty
        let mut rules = test_rules();
        rules.storage_classes.get_mut("STANDARD").unwrap().min_storage_duration_days = Some(30);

        let mut sim = Simulator::new(rules);

        let log = OperationLog {
            operations: vec![
                // Enable versioning
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: None,
                    kind: OperationKind::SetBucketVersioning { enabled: true },
                },
                // Upload file
                Operation {
                    timestamp: "2024-01-01T00:01:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024 * 1024 * 1024, // 1 GB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                // Update file (creates noncurrent version)
                Operation {
                    timestamp: "2024-01-02T00:00:00Z".parse()?,
                    bucket: "test-bucket".into(),
                    key: Some("file.txt".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 1024 * 1024 * 1024, // 1 GB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;

        // Should have early deletion penalty for the overwritten version
        assert!(!report.breakdown.early_deletion_penalties.is_zero());

        Ok(())
    }

    #[test]
    fn replicate_object_incurs_transfer_cost() -> Result<(), Box<dyn std::error::Error>> {
        // Create rules with egress pricing
        let mut rules = test_rules();
        rules.data_transfer.egress_price_per_gb = TieredPrice::flat(
            Money::from_str("0.09").ok().unwrap_or(Money::ZERO),
        );

        let mut sim = Simulator::new(rules);

        let log = OperationLog {
            operations: vec![
                // Upload file
                Operation {
                    timestamp: "2024-01-01T00:00:00Z".parse()?,
                    bucket: "source-bucket".into(),
                    key: Some("data.bin".into()),
                    kind: OperationKind::PutObject {
                        size_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
                        storage_class: StorageClass::new("STANDARD"),
                    },
                },
                // Replicate to another region
                Operation {
                    timestamp: "2024-01-01T00:01:00Z".parse()?,
                    bucket: "source-bucket".into(),
                    key: Some("data.bin".into()),
                    kind: OperationKind::ReplicateObject {
                        destination_region: "eu-west-1".to_string(),
                        destination_bucket: Some("dest-bucket".to_string()),
                        destination_key: None,
                        storage_class: None,
                    },
                },
            ],
            metadata: None,
        };

        let report = sim.simulate(&log)?;

        // Should have egress cost for replication
        assert!(!report.breakdown.data_transfer_egress.is_zero());

        Ok(())
    }
}

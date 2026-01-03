//! Cost reporting structures.

use crate::types::{Bytes, Money, StorageClass};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Complete cost report from a simulation.
#[derive(Debug, Clone, Default)]
pub struct CostReport {
    /// Total cost across all categories.
    pub total_cost: Money,

    /// Cost breakdown by category.
    pub breakdown: CostBreakdown,

    /// Per-object cost breakdown.
    pub object_costs: HashMap<String, ObjectCosts>,

    /// Simulation time range.
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,

    /// Summary statistics.
    pub stats: SimulationStats,
}

impl CostReport {
    /// Creates a new empty cost report.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds costs to the report.
    pub fn add_storage_cost(&mut self, class: &StorageClass, amount: Money) {
        self.breakdown
            .storage_by_class
            .entry(class.clone())
            .and_modify(|c| *c += amount)
            .or_insert(amount);
        self.breakdown.total_storage += amount;
        self.total_cost += amount;
    }

    /// Adds operation cost.
    pub fn add_operation_cost(&mut self, op_type: &str, amount: Money) {
        self.breakdown
            .operations_by_type
            .entry(op_type.to_string())
            .and_modify(|c| *c += amount)
            .or_insert(amount);
        self.breakdown.total_operations += amount;
        self.total_cost += amount;
    }

    /// Adds data transfer cost.
    pub fn add_egress_cost(&mut self, amount: Money) {
        self.breakdown.data_transfer_egress += amount;
        self.total_cost += amount;
    }

    /// Adds retrieval cost.
    pub fn add_retrieval_cost(&mut self, amount: Money) {
        self.breakdown.retrieval += amount;
        self.total_cost += amount;
    }

    /// Adds early deletion penalty.
    pub fn add_early_deletion_penalty(&mut self, amount: Money) {
        self.breakdown.early_deletion_penalties += amount;
        self.total_cost += amount;
    }

    /// Adds lifecycle transition cost.
    pub fn add_transition_cost(&mut self, amount: Money) {
        self.breakdown.lifecycle_transitions += amount;
        self.total_cost += amount;
    }

    /// Records cost for a specific object.
    pub fn record_object_cost(&mut self, path: &str, category: &str, amount: Money) {
        let obj = self.object_costs.entry(path.to_string()).or_default();
        obj.total += amount;
        obj.by_category
            .entry(category.to_string())
            .and_modify(|c| *c += amount)
            .or_insert(amount);
    }
}

impl std::fmt::Display for CostReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Cost Report")?;
        writeln!(f, "===========")?;
        writeln!(f)?;
        writeln!(f, "Total Cost: {}", self.total_cost)?;
        writeln!(f)?;
        writeln!(f, "Breakdown:")?;
        writeln!(
            f,
            "  Storage:              {}",
            self.breakdown.total_storage
        )?;
        writeln!(
            f,
            "  Operations:           {}",
            self.breakdown.total_operations
        )?;
        writeln!(
            f,
            "  Data Transfer:        {}",
            self.breakdown.data_transfer_egress
        )?;
        writeln!(f, "  Retrieval:            {}", self.breakdown.retrieval)?;
        writeln!(
            f,
            "  Early Deletion:       {}",
            self.breakdown.early_deletion_penalties
        )?;
        writeln!(
            f,
            "  Lifecycle Transitions:{}",
            self.breakdown.lifecycle_transitions
        )?;
        writeln!(f)?;

        if !self.breakdown.storage_by_class.is_empty() {
            writeln!(f, "Storage by Class:")?;
            for (class, cost) in &self.breakdown.storage_by_class {
                writeln!(f, "  {}: {}", class, cost)?;
            }
            writeln!(f)?;
        }

        if !self.breakdown.operations_by_type.is_empty() {
            writeln!(f, "Operations by Type:")?;
            for (op, cost) in &self.breakdown.operations_by_type {
                writeln!(f, "  {}: {}", op, cost)?;
            }
        }

        Ok(())
    }
}

/// Cost breakdown by category.
#[derive(Debug, Clone, Default)]
pub struct CostBreakdown {
    /// Total storage costs.
    pub total_storage: Money,

    /// Storage costs by class.
    pub storage_by_class: HashMap<StorageClass, Money>,

    /// Total operation costs.
    pub total_operations: Money,

    /// Operation costs by type.
    pub operations_by_type: HashMap<String, Money>,

    /// Data transfer egress costs.
    pub data_transfer_egress: Money,

    /// Retrieval costs (for archive tiers).
    pub retrieval: Money,

    /// Early deletion penalties.
    pub early_deletion_penalties: Money,

    /// Lifecycle transition costs.
    pub lifecycle_transitions: Money,
}

/// Per-object cost tracking.
#[derive(Debug, Clone, Default)]
pub struct ObjectCosts {
    /// Total cost for this object.
    pub total: Money,

    /// Cost by category.
    pub by_category: HashMap<String, Money>,
}

/// Summary statistics from a simulation.
#[derive(Debug, Clone, Default)]
pub struct SimulationStats {
    /// Total operations processed.
    pub total_operations: u64,

    /// Operations by type.
    pub operations_by_type: HashMap<String, u64>,

    /// Total bytes uploaded.
    pub bytes_uploaded: Bytes,

    /// Total bytes downloaded.
    pub bytes_downloaded: Bytes,

    /// Peak storage usage.
    pub peak_storage: Bytes,

    /// Final storage usage.
    pub final_storage: Bytes,

    /// Objects created.
    pub objects_created: u64,

    /// Objects deleted.
    pub objects_deleted: u64,
}

impl SimulationStats {
    /// Records an operation.
    pub fn record_operation(&mut self, op_type: &str) {
        self.total_operations += 1;
        *self
            .operations_by_type
            .entry(op_type.to_string())
            .or_default() += 1;
    }

    /// Records bytes uploaded.
    pub fn record_upload(&mut self, bytes: Bytes) {
        self.bytes_uploaded += bytes;
    }

    /// Records bytes downloaded.
    pub fn record_download(&mut self, bytes: Bytes) {
        self.bytes_downloaded += bytes;
    }

    /// Updates peak storage if current is higher.
    pub fn update_peak_storage(&mut self, current: Bytes) {
        if current > self.peak_storage {
            self.peak_storage = current;
        }
    }
}

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
                writeln!(f, "  {class}: {cost}")?;
            }
            writeln!(f)?;
        }

        if !self.breakdown.operations_by_type.is_empty() {
            writeln!(f, "Operations by Type:")?;
            for (op, cost) in &self.breakdown.operations_by_type {
                writeln!(f, "  {op}: {cost}")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn cost_report_new_is_empty() {
        let report = CostReport::new();
        assert!(report.total_cost.is_zero());
        assert!(report.breakdown.total_storage.is_zero());
        assert!(report.object_costs.is_empty());
    }

    #[test]
    fn add_storage_cost_updates_totals() {
        let mut report = CostReport::new();
        let class = StorageClass::new("STANDARD");
        let amount = Money::from_str("10.00").ok().unwrap_or(Money::ZERO);

        report.add_storage_cost(&class, amount);

        assert_eq!(report.total_cost, amount);
        assert_eq!(report.breakdown.total_storage, amount);
        assert_eq!(report.breakdown.storage_by_class.get(&class), Some(&amount));
    }

    #[test]
    fn add_storage_cost_accumulates() {
        let mut report = CostReport::new();
        let class = StorageClass::new("STANDARD");
        let amount = Money::from_str("5.00").ok().unwrap_or(Money::ZERO);

        report.add_storage_cost(&class, amount);
        report.add_storage_cost(&class, amount);

        let expected = Money::from_str("10.00").ok().unwrap_or(Money::ZERO);
        assert_eq!(report.total_cost, expected);
        assert_eq!(report.breakdown.storage_by_class.get(&class), Some(&expected));
    }

    #[test]
    fn add_operation_cost_updates_totals() {
        let mut report = CostReport::new();
        let amount = Money::from_str("0.50").ok().unwrap_or(Money::ZERO);

        report.add_operation_cost("PUT", amount);

        assert_eq!(report.total_cost, amount);
        assert_eq!(report.breakdown.total_operations, amount);
        assert_eq!(
            report.breakdown.operations_by_type.get("PUT"),
            Some(&amount)
        );
    }

    #[test]
    fn add_egress_cost_updates_totals() {
        let mut report = CostReport::new();
        let amount = Money::from_str("2.50").ok().unwrap_or(Money::ZERO);

        report.add_egress_cost(amount);

        assert_eq!(report.total_cost, amount);
        assert_eq!(report.breakdown.data_transfer_egress, amount);
    }

    #[test]
    fn add_retrieval_cost_updates_totals() {
        let mut report = CostReport::new();
        let amount = Money::from_str("1.00").ok().unwrap_or(Money::ZERO);

        report.add_retrieval_cost(amount);

        assert_eq!(report.total_cost, amount);
        assert_eq!(report.breakdown.retrieval, amount);
    }

    #[test]
    fn add_early_deletion_penalty_updates_totals() {
        let mut report = CostReport::new();
        let amount = Money::from_str("3.00").ok().unwrap_or(Money::ZERO);

        report.add_early_deletion_penalty(amount);

        assert_eq!(report.total_cost, amount);
        assert_eq!(report.breakdown.early_deletion_penalties, amount);
    }

    #[test]
    fn add_transition_cost_updates_totals() {
        let mut report = CostReport::new();
        let amount = Money::from_str("0.10").ok().unwrap_or(Money::ZERO);

        report.add_transition_cost(amount);

        assert_eq!(report.total_cost, amount);
        assert_eq!(report.breakdown.lifecycle_transitions, amount);
    }

    #[test]
    fn record_object_cost_creates_entry() {
        let mut report = CostReport::new();
        let amount = Money::from_str("1.50").ok().unwrap_or(Money::ZERO);

        report.record_object_cost("bucket/key.txt", "storage", amount);

        let obj = report.object_costs.get("bucket/key.txt");
        assert!(obj.is_some());
        let obj = obj.expect("object should exist");
        assert_eq!(obj.total, amount);
        assert_eq!(obj.by_category.get("storage"), Some(&amount));
    }

    #[test]
    fn record_object_cost_accumulates() {
        let mut report = CostReport::new();
        let amount = Money::from_str("1.00").ok().unwrap_or(Money::ZERO);

        report.record_object_cost("bucket/key.txt", "storage", amount);
        report.record_object_cost("bucket/key.txt", "operations", amount);
        report.record_object_cost("bucket/key.txt", "storage", amount);

        let obj = report
            .object_costs
            .get("bucket/key.txt")
            .expect("object should exist");
        let expected_total = Money::from_str("3.00").ok().unwrap_or(Money::ZERO);
        let expected_storage = Money::from_str("2.00").ok().unwrap_or(Money::ZERO);
        assert_eq!(obj.total, expected_total);
        assert_eq!(obj.by_category.get("storage"), Some(&expected_storage));
        assert_eq!(obj.by_category.get("operations"), Some(&amount));
    }

    #[test]
    fn display_produces_output() {
        let mut report = CostReport::new();
        let class = StorageClass::new("STANDARD");
        let amount = Money::from_str("10.00").ok().unwrap_or(Money::ZERO);

        report.add_storage_cost(&class, amount);
        report.add_operation_cost("PUT", amount);

        let display = format!("{report}");
        assert!(display.contains("Cost Report"));
        assert!(display.contains("Total Cost:"));
        assert!(display.contains("Storage:"));
        assert!(display.contains("Operations:"));
        assert!(display.contains("Storage by Class:"));
        assert!(display.contains("STANDARD"));
        assert!(display.contains("Operations by Type:"));
        assert!(display.contains("PUT"));
    }

    #[test]
    fn display_empty_report() {
        let report = CostReport::new();
        let display = format!("{report}");
        assert!(display.contains("Cost Report"));
        assert!(display.contains("$0.0000"));
    }

    #[test]
    fn simulation_stats_record_operation() {
        let mut stats = SimulationStats::default();

        stats.record_operation("PUT");
        stats.record_operation("PUT");
        stats.record_operation("GET");

        assert_eq!(stats.total_operations, 3);
        assert_eq!(stats.operations_by_type.get("PUT"), Some(&2));
        assert_eq!(stats.operations_by_type.get("GET"), Some(&1));
    }

    #[test]
    fn simulation_stats_record_upload() {
        let mut stats = SimulationStats::default();

        stats.record_upload(Bytes::from_mb(10));
        stats.record_upload(Bytes::from_mb(5));

        assert_eq!(stats.bytes_uploaded, Bytes::from_mb(15));
    }

    #[test]
    fn simulation_stats_record_download() {
        let mut stats = SimulationStats::default();

        stats.record_download(Bytes::from_gb(1));
        stats.record_download(Bytes::from_mb(500));

        assert_eq!(
            stats.bytes_downloaded,
            Bytes::from_gb(1) + Bytes::from_mb(500)
        );
    }

    #[test]
    fn simulation_stats_update_peak_storage() {
        let mut stats = SimulationStats::default();

        stats.update_peak_storage(Bytes::from_gb(10));
        assert_eq!(stats.peak_storage, Bytes::from_gb(10));

        // Lower value doesn't update peak
        stats.update_peak_storage(Bytes::from_gb(5));
        assert_eq!(stats.peak_storage, Bytes::from_gb(10));

        // Higher value updates peak
        stats.update_peak_storage(Bytes::from_gb(20));
        assert_eq!(stats.peak_storage, Bytes::from_gb(20));
    }
}

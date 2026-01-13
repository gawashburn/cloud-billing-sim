//! Core validation traits.

use crate::operations::OperationLog;
use crate::pricing::PricingRules;
use crate::types::Money;
use crate::validation::ValidationError;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::future::Future;

/// A provider that can validate simulated costs against real billing.
///
/// Implementations of this trait connect to real cloud storage services
/// to execute operations and compare actual costs with simulated costs.
pub trait ValidationProvider: Send + Sync {
    /// Executes a workload against the real cloud provider.
    ///
    /// This method performs the actual cloud operations and records
    /// the timing and any provider-specific metadata needed for
    /// cost comparison.
    fn execute_workload(
        &self,
        workload: &ValidationWorkload,
    ) -> impl Future<Output = Result<ExecutedWorkload, ValidationError>> + Send;

    /// Retrieves actual billing data for a time period.
    ///
    /// # Arguments
    ///
    /// * `start` - Start of the billing period
    /// * `end` - End of the billing period
    /// * `bucket` - Optional bucket filter
    ///
    /// # Note
    ///
    /// Billing data may be delayed by several hours to a day depending
    /// on the cloud provider. This method will return an error if data
    /// is not yet available.
    fn get_actual_costs(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        bucket: Option<&str>,
    ) -> impl Future<Output = Result<ActualCosts, ValidationError>> + Send;

    /// Validates a workload by executing it and comparing costs.
    ///
    /// This is a convenience method that:
    /// 1. Executes the workload
    /// 2. Runs the simulator with the same operations
    /// 3. Waits for billing data
    /// 4. Compares actual vs simulated costs
    fn validate_workload(
        &self,
        workload: &ValidationWorkload,
        rules: &PricingRules,
    ) -> impl Future<Output = Result<ValidationResult, ValidationError>> + Send;

    /// Returns the provider name (e.g., "AWS S3", "Backblaze B2").
    fn provider_name(&self) -> &str;

    /// Returns the region or endpoint being validated.
    fn region(&self) -> &str;

    /// Cleans up test resources.
    ///
    /// This should delete any objects created during validation.
    fn cleanup(&self) -> impl Future<Output = Result<(), ValidationError>> + Send;
}

/// A workload to be validated.
#[derive(Debug, Clone)]
pub struct ValidationWorkload {
    /// Operations to execute.
    pub operations: OperationLog,

    /// Test bucket name.
    pub bucket: String,

    /// Key prefix for test objects (to isolate tests).
    pub key_prefix: String,

    /// Whether to clean up after execution.
    pub cleanup_after: bool,

    /// Timeout for the entire workload in seconds.
    pub timeout_seconds: u64,
}

impl ValidationWorkload {
    /// Creates a new validation workload.
    #[must_use]
    pub fn new(operations: OperationLog, bucket: impl Into<String>) -> Self {
        Self {
            operations,
            bucket: bucket.into(),
            key_prefix: format!("validation-{}/", Utc::now().format("%Y%m%d-%H%M%S")),
            cleanup_after: true,
            timeout_seconds: 300,
        }
    }

    /// Sets a custom key prefix.
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    /// Disables cleanup after execution.
    #[must_use]
    pub fn keep_objects(mut self) -> Self {
        self.cleanup_after = false;
        self
    }

    /// Sets a custom timeout.
    #[must_use]
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }
}

/// The result of executing a workload.
#[derive(Debug, Clone)]
pub struct ExecutedWorkload {
    /// When execution started.
    pub start_time: DateTime<Utc>,

    /// When execution completed.
    pub end_time: DateTime<Utc>,

    /// Number of operations executed.
    pub operations_executed: usize,

    /// Total bytes uploaded.
    pub bytes_uploaded: u64,

    /// Total bytes downloaded.
    pub bytes_downloaded: u64,

    /// Keys of created objects (for cleanup).
    pub created_objects: Vec<String>,

    /// Provider-specific metadata.
    pub metadata: HashMap<String, String>,
}

/// Actual costs from the cloud provider.
#[derive(Debug, Clone, Default)]
pub struct ActualCosts {
    /// Total cost across all categories.
    pub total: Money,

    /// Storage costs.
    pub storage: Money,

    /// Operation costs (PUT, GET, LIST, etc.).
    pub operations: Money,

    /// Data transfer/egress costs.
    pub data_transfer: Money,

    /// Retrieval costs (for archive classes).
    pub retrieval: Money,

    /// Cost breakdown by category.
    pub by_category: HashMap<String, Money>,

    /// Time range for these costs.
    pub period: Option<(DateTime<Utc>, DateTime<Utc>)>,

    /// Whether the data is complete or partial.
    pub is_complete: bool,

    /// Provider-specific notes.
    pub notes: Vec<String>,
}

impl ActualCosts {
    /// Creates empty costs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks if any cost data was retrieved.
    #[must_use]
    pub fn has_data(&self) -> bool {
        !self.total.is_zero() || !self.by_category.is_empty()
    }
}

/// Comparison between simulated and actual costs.
#[derive(Debug, Clone)]
pub struct CostComparison {
    /// Category name.
    pub category: String,

    /// Simulated cost.
    pub simulated: Money,

    /// Actual cost from provider.
    pub actual: Money,

    /// Difference (actual - simulated).
    pub difference: Money,

    /// Percentage difference.
    pub percentage_diff: f64,
}

impl CostComparison {
    /// Creates a new cost comparison.
    #[must_use]
    pub fn new(category: impl Into<String>, simulated: Money, actual: Money) -> Self {
        let difference = if actual >= simulated {
            actual - simulated
        } else {
            simulated - actual
        };

        let percentage_diff = if simulated.is_zero() {
            if actual.is_zero() {
                0.0
            } else {
                100.0
            }
        } else {
            let sim_f64: f64 = simulated.as_decimal().try_into().unwrap_or(0.0);
            let diff_f64: f64 = difference.as_decimal().try_into().unwrap_or(0.0);
            (diff_f64 / sim_f64) * 100.0
        };

        Self {
            category: category.into(),
            simulated,
            actual,
            difference,
            percentage_diff: percentage_diff.abs(),
        }
    }

    /// Returns true if the costs match within a tolerance.
    #[must_use]
    pub fn is_within_tolerance(&self, tolerance_percent: f64) -> bool {
        self.percentage_diff <= tolerance_percent
    }
}

/// Complete validation result.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Provider that was validated.
    pub provider: String,

    /// Region that was validated.
    pub region: String,

    /// When validation was performed.
    pub timestamp: DateTime<Utc>,

    /// Total simulated cost.
    pub simulated_total: Money,

    /// Total actual cost.
    pub actual_total: Money,

    /// Cost comparisons by category.
    pub comparisons: Vec<CostComparison>,

    /// Workload execution details.
    pub execution: ExecutedWorkload,

    /// Overall accuracy percentage (100 = perfect match).
    pub accuracy_percent: f64,

    /// Whether validation passed (within tolerance).
    pub passed: bool,

    /// Tolerance used for pass/fail determination.
    pub tolerance_percent: f64,

    /// Any warnings or notes.
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Creates a new validation result.
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        region: impl Into<String>,
        simulated_total: Money,
        actual_total: Money,
        execution: ExecutedWorkload,
    ) -> Self {
        let comparison = CostComparison::new("total", simulated_total, actual_total);
        let accuracy = 100.0 - comparison.percentage_diff.min(100.0);

        Self {
            provider: provider.into(),
            region: region.into(),
            timestamp: Utc::now(),
            simulated_total,
            actual_total,
            comparisons: vec![comparison],
            execution,
            accuracy_percent: accuracy,
            passed: false,
            tolerance_percent: 5.0,
            warnings: Vec::new(),
        }
    }

    /// Sets the tolerance and updates pass/fail status.
    #[must_use]
    pub fn with_tolerance(mut self, tolerance_percent: f64) -> Self {
        self.tolerance_percent = tolerance_percent;
        self.passed = self.comparisons.iter().all(|c| c.is_within_tolerance(tolerance_percent));
        self
    }

    /// Adds a category comparison.
    pub fn add_comparison(&mut self, comparison: CostComparison) {
        self.comparisons.push(comparison);
        // Update accuracy based on total comparison
        if let Some(total) = self.comparisons.iter().find(|c| c.category == "total") {
            self.accuracy_percent = 100.0 - total.percentage_diff.min(100.0);
        }
    }

    /// Adds a warning.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Returns true if the validation passed.
    #[must_use]
    pub fn is_passing(&self) -> bool {
        self.passed
    }

    /// Returns the overall accuracy percentage.
    #[must_use]
    pub fn accuracy(&self) -> f64 {
        self.accuracy_percent
    }
}

impl std::fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Validation Result for {} ({})", self.provider, self.region)?;
        writeln!(f, "===========================================")?;
        writeln!(f, "Timestamp: {}", self.timestamp)?;
        writeln!(f)?;
        writeln!(f, "Costs:")?;
        writeln!(f, "  Simulated: {}", self.simulated_total)?;
        writeln!(f, "  Actual:    {}", self.actual_total)?;
        writeln!(f)?;
        writeln!(f, "Accuracy: {:.2}%", self.accuracy_percent)?;
        writeln!(
            f,
            "Status: {}",
            if self.passed { "PASSED" } else { "FAILED" }
        )?;
        writeln!(f)?;

        if !self.comparisons.is_empty() {
            writeln!(f, "Breakdown:")?;
            for comp in &self.comparisons {
                writeln!(
                    f,
                    "  {}: {} vs {} ({:.2}% diff)",
                    comp.category, comp.simulated, comp.actual, comp.percentage_diff
                )?;
            }
        }

        if !self.warnings.is_empty() {
            writeln!(f)?;
            writeln!(f, "Warnings:")?;
            for warning in &self.warnings {
                writeln!(f, "  - {warning}")?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn cost_comparison_calculates_difference() {
        let simulated = Money::from_str("10.00").ok().unwrap_or(Money::ZERO);
        let actual = Money::from_str("10.50").ok().unwrap_or(Money::ZERO);

        let comp = CostComparison::new("storage", simulated, actual);

        assert_eq!(comp.category, "storage");
        assert!((comp.percentage_diff - 5.0).abs() < 0.1);
    }

    #[test]
    fn cost_comparison_handles_zero() {
        let comp = CostComparison::new("empty", Money::ZERO, Money::ZERO);
        assert_eq!(comp.percentage_diff, 0.0);
    }

    #[test]
    fn cost_comparison_tolerance_check() {
        let simulated = Money::from_str("100.00").ok().unwrap_or(Money::ZERO);
        let actual = Money::from_str("103.00").ok().unwrap_or(Money::ZERO);

        let comp = CostComparison::new("ops", simulated, actual);

        assert!(comp.is_within_tolerance(5.0));
        assert!(!comp.is_within_tolerance(2.0));
    }

    #[test]
    fn validation_workload_defaults() {
        let log = OperationLog::new();
        let workload = ValidationWorkload::new(log, "test-bucket");

        assert_eq!(workload.bucket, "test-bucket");
        assert!(workload.cleanup_after);
        assert_eq!(workload.timeout_seconds, 300);
    }

    #[test]
    fn validation_workload_builder() {
        let log = OperationLog::new();
        let workload = ValidationWorkload::new(log, "bucket")
            .with_prefix("custom/")
            .keep_objects()
            .with_timeout(600);

        assert_eq!(workload.key_prefix, "custom/");
        assert!(!workload.cleanup_after);
        assert_eq!(workload.timeout_seconds, 600);
    }

    #[test]
    fn actual_costs_default_is_empty() {
        let costs = ActualCosts::new();
        assert!(costs.total.is_zero());
        assert!(!costs.has_data());
        assert!(!costs.is_complete);
    }

    #[test]
    fn validation_result_display() {
        let execution = ExecutedWorkload {
            start_time: Utc::now(),
            end_time: Utc::now(),
            operations_executed: 10,
            bytes_uploaded: 1000,
            bytes_downloaded: 500,
            created_objects: vec![],
            metadata: HashMap::new(),
        };

        let result = ValidationResult::new(
            "AWS S3",
            "us-east-1",
            Money::from_str("10.00").ok().unwrap_or(Money::ZERO),
            Money::from_str("10.25").ok().unwrap_or(Money::ZERO),
            execution,
        )
        .with_tolerance(5.0);

        let display = format!("{result}");
        assert!(display.contains("AWS S3"));
        assert!(display.contains("us-east-1"));
        assert!(display.contains("PASSED"));
    }
}

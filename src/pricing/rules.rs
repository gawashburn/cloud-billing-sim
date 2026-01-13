//! Pricing rule structures.

use crate::types::{Bytes, Money, StorageClass};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

use super::TieredPrice;

/// Deserializes a string as Money.
fn deserialize_money<'de, D>(deserializer: D) -> Result<Money, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Money::from_str(&s).map_err(serde::de::Error::custom)
}

/// Deserializes an optional string as Money.
fn deserialize_option_money<'de, D>(deserializer: D) -> Result<Option<Money>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    opt.map_or(Ok(None), |s| {
        Money::from_str(&s)
            .map(Some)
            .map_err(serde::de::Error::custom)
    })
}

/// Complete pricing rules for a cloud storage provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingRules {
    /// Provider metadata.
    pub provider: ProviderInfo,

    /// Storage class definitions and pricing.
    pub storage_classes: HashMap<String, StorageClassRules>,

    /// Operation pricing (may vary by storage class).
    #[serde(default)]
    pub operations: HashMap<String, OperationRules>,

    /// Data transfer pricing.
    #[serde(default)]
    pub data_transfer: DataTransferRules,

    /// Lifecycle transition costs.
    #[serde(default)]
    pub lifecycle_transitions: HashMap<String, LifecycleTransitionRules>,
}

impl PricingRules {
    /// Gets the storage class rules for a given class.
    #[must_use]
    pub fn get_storage_class(&self, class: &StorageClass) -> Option<&StorageClassRules> {
        self.storage_classes.get(class.as_str())
    }

    /// Gets the operation rules for a storage class, with fallback to default.
    #[must_use]
    pub fn get_operations(&self, class: &StorageClass) -> Option<&OperationRules> {
        self.operations
            .get(class.as_str())
            .or_else(|| self.operations.get("DEFAULT"))
    }

    /// Gets the lifecycle transition cost between two storage classes.
    #[must_use]
    pub fn get_transition_cost(
        &self,
        from: &StorageClass,
        to: &StorageClass,
    ) -> Option<&LifecycleTransitionRules> {
        let key = format!("{}_to_{}", from.as_str(), to.as_str());
        self.lifecycle_transitions.get(&key)
    }
}

/// Provider metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider name (e.g., "aws-s3", "backblaze-b2").
    pub name: String,

    /// Region identifier (e.g., "us-east-1").
    #[serde(default)]
    pub region: Option<String>,

    /// Pricing version or effective date.
    #[serde(default)]
    pub version: Option<String>,

    /// Currency code (defaults to USD).
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "USD".to_string()
}

/// Pricing rules for a specific storage class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageClassRules {
    /// Price per GB per month (may be tiered).
    #[serde(default)]
    pub storage_price_per_gb_month: TieredPrice,

    /// Minimum billable object size in bytes.
    /// Objects smaller than this are billed as if they were this size.
    #[serde(default)]
    pub min_billable_size_bytes: Option<u64>,

    /// Minimum storage duration in days.
    /// Objects deleted before this incur early deletion charges.
    #[serde(default)]
    pub min_storage_duration_days: Option<u32>,

    /// Additional metadata overhead in bytes (e.g., Glacier metadata).
    /// This is added to each object's billable size.
    #[serde(default)]
    pub metadata_overhead_bytes: Option<u64>,

    /// Base retrieval cost per GB (for IA/archive classes).
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub retrieval_price_per_gb: Option<Money>,

    /// Retrieval tier options (for Glacier-like classes).
    #[serde(default)]
    pub retrieval_tiers: Vec<RetrievalTier>,

    /// Whether this class supports intelligent tiering monitoring.
    #[serde(default)]
    pub intelligent_tiering: bool,

    /// Monitoring cost per 1000 objects per month (for intelligent tiering).
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub monitoring_price_per_1000_objects: Option<Money>,
}

impl StorageClassRules {
    /// Calculates the billable size for an object, accounting for minimum size and overhead.
    #[must_use]
    pub fn billable_size(&self, actual_size: Bytes) -> Bytes {
        let min_size = self.min_billable_size_bytes.map_or(Bytes::ZERO, Bytes::new);
        let overhead = self.metadata_overhead_bytes.map_or(Bytes::ZERO, Bytes::new);
        actual_size.max(min_size) + overhead
    }

    /// Calculates storage cost for a given size and duration fraction.
    ///
    /// `duration_fraction` is the fraction of a month (e.g., 0.5 for half a month).
    #[must_use]
    pub fn calculate_storage_cost(&self, size: Bytes, duration_fraction: Decimal) -> Money {
        let billable = self.billable_size(size);
        let gb = billable.as_gb_decimal();
        let monthly_cost = self.storage_price_per_gb_month.calculate_cost(gb);
        monthly_cost * duration_fraction
    }

    /// Calculates early deletion penalty if applicable.
    #[must_use]
    pub fn early_deletion_cost(&self, size: Bytes, days_stored: u32) -> Money {
        let Some(min_days) = self.min_storage_duration_days else {
            return Money::ZERO;
        };

        if days_stored >= min_days {
            return Money::ZERO;
        }

        // Pro-rated charge for remaining days
        let remaining_days = min_days - days_stored;
        let fraction = Decimal::from(remaining_days) / Decimal::from(30); // Approximate month
        self.calculate_storage_cost(size, fraction)
    }

    /// Gets the retrieval cost per GB for a given tier.
    #[must_use]
    pub fn get_retrieval_cost(&self, tier: Option<&str>) -> Money {
        tier.map_or_else(
            || self.retrieval_price_per_gb.unwrap_or(Money::ZERO),
            |tier_name| {
                self.retrieval_tiers
                    .iter()
                    .find(|t| t.name.eq_ignore_ascii_case(tier_name))
                    .map_or(Money::ZERO, |t| t.price_per_gb)
            },
        )
    }
}

/// A retrieval speed tier (e.g., expedited, standard, bulk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTier {
    /// Tier name (e.g., "expedited", "standard", "bulk").
    pub name: String,

    /// Price per GB retrieved.
    #[serde(deserialize_with = "deserialize_money")]
    pub price_per_gb: Money,

    /// Price per 1000 retrieval requests.
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub price_per_1000_requests: Option<Money>,
}

/// Operation pricing for a storage class.
// The `_per_1000` suffix is intentional - these are pricing rates per 1000 requests.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperationRules {
    /// PUT, COPY, POST request price per 1000 requests.
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub put_per_1000: Option<Money>,

    /// GET, SELECT request price per 1000 requests.
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub get_per_1000: Option<Money>,

    /// LIST request price per 1000 requests.
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub list_per_1000: Option<Money>,

    /// DELETE request price per 1000 requests (usually free).
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub delete_per_1000: Option<Money>,

    /// HEAD request price per 1000 requests.
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub head_per_1000: Option<Money>,

    /// Lifecycle transition request price per 1000 requests.
    #[serde(default, deserialize_with = "deserialize_option_money")]
    pub lifecycle_transition_per_1000: Option<Money>,
}

impl OperationRules {
    /// Gets the cost for a single operation of the given type.
    #[must_use]
    pub fn cost_for_operation(&self, op_type: OperationType) -> Money {
        let per_1000 = match op_type {
            OperationType::Put | OperationType::Copy | OperationType::Post => self.put_per_1000,
            OperationType::Get | OperationType::Select => self.get_per_1000,
            OperationType::List => self.list_per_1000,
            OperationType::Delete => self.delete_per_1000,
            OperationType::Head => self.head_per_1000,
            OperationType::LifecycleTransition => self.lifecycle_transition_per_1000,
        };

        per_1000.map_or(Money::ZERO, |p| p * Decimal::new(1, 3)) // Divide by 1000
    }
}

/// Types of storage operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    /// PUT request (upload object).
    Put,
    /// COPY request (copy object).
    Copy,
    /// POST request (e.g., multipart upload).
    Post,
    /// GET request (download object).
    Get,
    /// SELECT request (query object content).
    Select,
    /// LIST request (list objects).
    List,
    /// DELETE request (remove object).
    Delete,
    /// HEAD request (get object metadata).
    Head,
    /// Lifecycle transition (move between storage classes).
    LifecycleTransition,
}

/// Data transfer pricing rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataTransferRules {
    /// Data ingress (upload) price per GB.
    #[serde(default)]
    pub ingress_price_per_gb: TieredPrice,

    /// Data egress (download) price per GB.
    #[serde(default)]
    pub egress_price_per_gb: TieredPrice,

    /// Free egress allowance per month in GB.
    #[serde(default)]
    pub free_egress_gb_per_month: Option<u64>,

    /// Free egress as multiple of storage (e.g., Backblaze 3x rule).
    #[serde(default)]
    pub free_egress_storage_multiplier: Option<Decimal>,
}

impl DataTransferRules {
    /// Calculates egress cost, accounting for free tier if applicable.
    #[must_use]
    pub fn calculate_egress_cost(&self, gb: Decimal, storage_gb: Decimal) -> Money {
        let free_from_allowance = self
            .free_egress_gb_per_month
            .map_or(Decimal::ZERO, Decimal::from);

        let free_from_multiplier = self
            .free_egress_storage_multiplier
            .map_or(Decimal::ZERO, |m| storage_gb * m);

        let free_total = free_from_allowance + free_from_multiplier;
        let billable = (gb - free_total).max(Decimal::ZERO);

        self.egress_price_per_gb.calculate_cost(billable)
    }
}

/// Key for lifecycle transition pricing lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleTransitionKey {
    /// Source storage class.
    pub from: StorageClass,
    /// Destination storage class.
    pub to: StorageClass,
}

/// Lifecycle transition pricing rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransitionRules {
    /// Cost per 1000 transition requests.
    #[serde(deserialize_with = "deserialize_money")]
    pub per_1000_requests: Money,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_storage_class_rules() -> StorageClassRules {
        StorageClassRules {
            storage_price_per_gb_month: TieredPrice::flat(
                Money::from_str("0.023").ok().unwrap_or(Money::ZERO),
            ),
            min_billable_size_bytes: Some(128 * 1024), // 128 KB
            min_storage_duration_days: Some(30),
            metadata_overhead_bytes: None,
            retrieval_price_per_gb: Some(Money::from_str("0.01").ok().unwrap_or(Money::ZERO)),
            retrieval_tiers: vec![],
            intelligent_tiering: false,
            monitoring_price_per_1000_objects: None,
        }
    }

    fn sample_storage_class_rules_with_tiers() -> StorageClassRules {
        StorageClassRules {
            storage_price_per_gb_month: TieredPrice::flat(
                Money::from_str("0.023").ok().unwrap_or(Money::ZERO),
            ),
            min_billable_size_bytes: None,
            min_storage_duration_days: None,
            metadata_overhead_bytes: Some(32 * 1024), // 32 KB overhead
            retrieval_price_per_gb: None,
            retrieval_tiers: vec![
                RetrievalTier {
                    name: "expedited".to_string(),
                    price_per_gb: Money::from_str("0.03").ok().unwrap_or(Money::ZERO),
                    price_per_1000_requests: None,
                },
                RetrievalTier {
                    name: "standard".to_string(),
                    price_per_gb: Money::from_str("0.01").ok().unwrap_or(Money::ZERO),
                    price_per_1000_requests: None,
                },
                RetrievalTier {
                    name: "bulk".to_string(),
                    price_per_gb: Money::from_str("0.0025").ok().unwrap_or(Money::ZERO),
                    price_per_1000_requests: None,
                },
            ],
            intelligent_tiering: false,
            monitoring_price_per_1000_objects: None,
        }
    }

    #[test]
    fn billable_size_respects_minimum() {
        let rules = sample_storage_class_rules();

        // Small object gets billed as minimum
        let small = Bytes::new(1000);
        assert_eq!(rules.billable_size(small), Bytes::from_kb(128));

        // Large object billed at actual size
        let large = Bytes::from_mb(1);
        assert_eq!(rules.billable_size(large), Bytes::from_mb(1));
    }

    #[test]
    fn billable_size_adds_metadata_overhead() {
        let rules = sample_storage_class_rules_with_tiers();

        // Object size plus 32 KB overhead
        let size = Bytes::from_mb(1);
        let billable = rules.billable_size(size);
        assert_eq!(billable, Bytes::from_mb(1) + Bytes::from_kb(32));
    }

    #[test]
    fn billable_size_no_minimum_no_overhead() {
        let rules = StorageClassRules {
            storage_price_per_gb_month: TieredPrice::default(),
            min_billable_size_bytes: None,
            min_storage_duration_days: None,
            metadata_overhead_bytes: None,
            retrieval_price_per_gb: None,
            retrieval_tiers: vec![],
            intelligent_tiering: false,
            monitoring_price_per_1000_objects: None,
        };

        let size = Bytes::new(100);
        assert_eq!(rules.billable_size(size), size);
    }

    #[test]
    fn early_deletion_penalty_calculation() {
        let rules = sample_storage_class_rules();

        // No penalty after min duration
        let cost = rules.early_deletion_cost(Bytes::from_gb(1), 30);
        assert!(cost.is_zero());

        // Penalty for early deletion
        let cost = rules.early_deletion_cost(Bytes::from_gb(1), 15);
        assert!(!cost.is_zero());
    }

    #[test]
    fn early_deletion_no_min_duration() {
        let mut rules = sample_storage_class_rules();
        rules.min_storage_duration_days = None;

        // No penalty when no min duration
        let cost = rules.early_deletion_cost(Bytes::from_gb(1), 1);
        assert!(cost.is_zero());
    }

    #[test]
    fn retrieval_cost_with_base_price() {
        let rules = sample_storage_class_rules();

        // No tier specified - use base retrieval price
        let cost = rules.get_retrieval_cost(None);
        assert_eq!(cost, Money::from_str("0.01").ok().unwrap_or(Money::ZERO));
    }

    #[test]
    fn retrieval_cost_with_tier() {
        let rules = sample_storage_class_rules_with_tiers();

        // Get expedited tier cost
        let cost = rules.get_retrieval_cost(Some("expedited"));
        assert_eq!(cost, Money::from_str("0.03").ok().unwrap_or(Money::ZERO));

        // Get standard tier cost (case insensitive)
        let cost = rules.get_retrieval_cost(Some("STANDARD"));
        assert_eq!(cost, Money::from_str("0.01").ok().unwrap_or(Money::ZERO));

        // Get bulk tier cost
        let cost = rules.get_retrieval_cost(Some("bulk"));
        assert_eq!(cost, Money::from_str("0.0025").ok().unwrap_or(Money::ZERO));

        // Unknown tier returns zero
        let cost = rules.get_retrieval_cost(Some("unknown"));
        assert!(cost.is_zero());
    }

    #[test]
    fn retrieval_cost_no_tiers_no_base() {
        let rules = StorageClassRules {
            storage_price_per_gb_month: TieredPrice::default(),
            min_billable_size_bytes: None,
            min_storage_duration_days: None,
            metadata_overhead_bytes: None,
            retrieval_price_per_gb: None,
            retrieval_tiers: vec![],
            intelligent_tiering: false,
            monitoring_price_per_1000_objects: None,
        };

        let cost = rules.get_retrieval_cost(None);
        assert!(cost.is_zero());
    }

    #[test]
    fn storage_cost_calculation() {
        let rules = sample_storage_class_rules();

        // 1 GB for half a month
        let cost = rules.calculate_storage_cost(Bytes::from_gb(1), Decimal::new(5, 1));
        // 1 GB * $0.023 * 0.5 = $0.0115
        assert!(!cost.is_zero());
    }

    #[test]
    fn operation_rules_cost_for_operation() {
        let rules = OperationRules {
            put_per_1000: Some(Money::from_str("5.00").ok().unwrap_or(Money::ZERO)),
            get_per_1000: Some(Money::from_str("0.40").ok().unwrap_or(Money::ZERO)),
            list_per_1000: Some(Money::from_str("5.00").ok().unwrap_or(Money::ZERO)),
            delete_per_1000: None, // Free
            head_per_1000: Some(Money::from_str("0.40").ok().unwrap_or(Money::ZERO)),
            lifecycle_transition_per_1000: Some(
                Money::from_str("10.00").ok().unwrap_or(Money::ZERO),
            ),
        };

        // PUT operation cost: $5.00 / 1000 = $0.005
        let put_cost = rules.cost_for_operation(OperationType::Put);
        assert!(!put_cost.is_zero());

        // COPY operation cost (same as PUT)
        let copy_cost = rules.cost_for_operation(OperationType::Copy);
        assert_eq!(copy_cost, put_cost);

        // POST operation cost (same as PUT)
        let post_cost = rules.cost_for_operation(OperationType::Post);
        assert_eq!(post_cost, put_cost);

        // GET operation cost
        let get_cost = rules.cost_for_operation(OperationType::Get);
        assert!(!get_cost.is_zero());

        // SELECT operation cost (same as GET)
        let select_cost = rules.cost_for_operation(OperationType::Select);
        assert_eq!(select_cost, get_cost);

        // LIST operation cost
        let list_cost = rules.cost_for_operation(OperationType::List);
        assert!(!list_cost.is_zero());

        // DELETE operation cost (free)
        let delete_cost = rules.cost_for_operation(OperationType::Delete);
        assert!(delete_cost.is_zero());

        // HEAD operation cost
        let head_cost = rules.cost_for_operation(OperationType::Head);
        assert!(!head_cost.is_zero());

        // Lifecycle transition cost
        let transition_cost = rules.cost_for_operation(OperationType::LifecycleTransition);
        assert!(!transition_cost.is_zero());
    }

    #[test]
    fn data_transfer_egress_cost_no_free_tier() {
        let rules = DataTransferRules {
            ingress_price_per_gb: TieredPrice::default(),
            egress_price_per_gb: TieredPrice::flat(
                Money::from_str("0.09").ok().unwrap_or(Money::ZERO),
            ),
            free_egress_gb_per_month: None,
            free_egress_storage_multiplier: None,
        };

        // 100 GB egress at $0.09/GB = $9.00
        let cost = rules.calculate_egress_cost(Decimal::from(100), Decimal::ZERO);
        assert_eq!(cost, Money::from_str("9.00").ok().unwrap_or(Money::ZERO));
    }

    #[test]
    fn data_transfer_egress_cost_with_free_allowance() {
        let rules = DataTransferRules {
            ingress_price_per_gb: TieredPrice::default(),
            egress_price_per_gb: TieredPrice::flat(
                Money::from_str("0.09").ok().unwrap_or(Money::ZERO),
            ),
            free_egress_gb_per_month: Some(100), // 100 GB free
            free_egress_storage_multiplier: None,
        };

        // 50 GB egress with 100 GB free = $0
        let cost = rules.calculate_egress_cost(Decimal::from(50), Decimal::ZERO);
        assert!(cost.is_zero());

        // 150 GB egress with 100 GB free = 50 GB * $0.09 = $4.50
        let cost = rules.calculate_egress_cost(Decimal::from(150), Decimal::ZERO);
        assert_eq!(cost, Money::from_str("4.50").ok().unwrap_or(Money::ZERO));
    }

    #[test]
    fn data_transfer_egress_cost_with_storage_multiplier() {
        let rules = DataTransferRules {
            ingress_price_per_gb: TieredPrice::default(),
            egress_price_per_gb: TieredPrice::flat(
                Money::from_str("0.01").ok().unwrap_or(Money::ZERO),
            ),
            free_egress_gb_per_month: None,
            free_egress_storage_multiplier: Some(Decimal::from(3)), // 3x storage free
        };

        // 100 GB storage = 300 GB free egress
        // 200 GB egress with 300 GB free = $0
        let cost = rules.calculate_egress_cost(Decimal::from(200), Decimal::from(100));
        assert!(cost.is_zero());

        // 400 GB egress with 300 GB free = 100 GB * $0.01 = $1.00
        let cost = rules.calculate_egress_cost(Decimal::from(400), Decimal::from(100));
        assert_eq!(cost, Money::from_str("1.00").ok().unwrap_or(Money::ZERO));
    }

    #[test]
    fn data_transfer_combined_free_tiers() {
        let rules = DataTransferRules {
            ingress_price_per_gb: TieredPrice::default(),
            egress_price_per_gb: TieredPrice::flat(
                Money::from_str("0.10").ok().unwrap_or(Money::ZERO),
            ),
            free_egress_gb_per_month: Some(100),
            free_egress_storage_multiplier: Some(Decimal::from(2)),
        };

        // 50 GB storage = 100 GB from multiplier + 100 GB allowance = 200 GB free
        // 250 GB egress = 50 GB billable * $0.10 = $5.00
        let cost = rules.calculate_egress_cost(Decimal::from(250), Decimal::from(50));
        assert_eq!(cost, Money::from_str("5.00").ok().unwrap_or(Money::ZERO));
    }
}

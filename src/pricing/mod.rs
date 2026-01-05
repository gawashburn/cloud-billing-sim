//! Pricing rules DSL for cloud object storage.
//!
//! This module defines the domain-specific language for describing
//! cloud storage pricing. Rules are parsed from TOML files.
//!
//! # Example Pricing Rules
//!
//! ```toml
//! [provider]
//! name = "aws-s3"
//! region = "us-east-1"
//!
//! [storage_classes.STANDARD]
//! storage_price_per_gb_month = [
//!     { up_to_gb = 50000, price = "0.023" },
//!     { price = "0.021" }
//! ]
//!
//! [operations.STANDARD]
//! put_per_1000 = "0.005"
//! get_per_1000 = "0.0004"
//! ```

mod error;
mod rules;
mod tiered;

pub use error::PricingError;
pub use rules::{
    DataTransferRules, LifecycleTransitionKey, OperationRules, OperationType, PricingRules,
    ProviderInfo, RetrievalTier, StorageClassRules,
};
pub use tiered::{PriceTier, TieredPrice};

use std::path::Path;

/// Loads pricing rules from a TOML file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
///
/// # Examples
///
/// ```no_run
/// use cloud_billing_sim::pricing::load_rules;
///
/// let rules = load_rules("pricing/aws-s3-us-east-1.toml")?;
/// # Ok::<(), cloud_billing_sim::pricing::PricingError>(())
/// ```
pub fn load_rules(path: impl AsRef<Path>) -> Result<PricingRules, PricingError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| PricingError::Io(path.as_ref().to_path_buf(), e))?;
    parse_rules(&content)
}

/// Parses pricing rules from a TOML string.
///
/// # Errors
///
/// Returns an error if the TOML is invalid or missing required fields.
pub fn parse_rules(toml_content: &str) -> Result<PricingRules, PricingError> {
    toml::from_str(toml_content).map_err(PricingError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rules_valid_toml() {
        let toml = r#"
            [provider]
            name = "test-provider"
            currency = "USD"

            [storage_classes.STANDARD]
            storage_price_per_gb_month = "0.023"

            [operations.DEFAULT]
            put_per_1000 = "0.005"
            get_per_1000 = "0.0004"

            [data_transfer]
            ingress_price_per_gb = "0"
            egress_price_per_gb = "0.09"
        "#;

        let rules = parse_rules(toml).expect("should parse valid TOML");
        assert_eq!(rules.provider.name, "test-provider");
        assert!(rules.storage_classes.contains_key("STANDARD"));
    }

    #[test]
    fn parse_rules_invalid_toml() {
        let toml = "not valid toml [[[";
        let result = parse_rules(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rules_minimal() {
        let toml = r#"
            [provider]
            name = "minimal"

            [storage_classes.STANDARD]
            storage_price_per_gb_month = "0.01"

            [operations.DEFAULT]

            [data_transfer]
        "#;

        let rules = parse_rules(toml).expect("should parse minimal config");
        assert_eq!(rules.provider.name, "minimal");
    }
}

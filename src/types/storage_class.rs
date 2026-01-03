//! Storage class identifiers.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifies a storage class/tier within a provider's offering.
///
/// Storage classes are provider-specific strings that identify different
/// storage tiers with varying cost, durability, and access characteristics.
///
/// # Examples
///
/// ```
/// use cloud_billing_sim::types::StorageClass;
///
/// let standard = StorageClass::new("STANDARD");
/// let glacier = StorageClass::new("GLACIER");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorageClass(String);

impl StorageClass {
    /// Creates a new storage class identifier.
    ///
    /// The identifier is normalized to uppercase for consistent matching.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into().to_uppercase())
    }

    /// Returns the storage class name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StorageClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for StorageClass {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for StorageClass {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_to_uppercase() {
        let sc = StorageClass::new("standard");
        assert_eq!(sc.as_str(), "STANDARD");
    }

    #[test]
    fn equality_is_case_insensitive() {
        let a = StorageClass::new("Standard");
        let b = StorageClass::new("STANDARD");
        assert_eq!(a, b);
    }
}

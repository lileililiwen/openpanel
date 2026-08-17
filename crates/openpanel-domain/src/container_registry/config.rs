//! Registry configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::retention::RetentionPolicy;

/// Global registry configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryConfig {
    /// Storage root for blob / manifest data.
    pub storage_root: PathBuf,
    /// Retention policy applied after each push.
    pub retention: RetentionPolicy,
    /// `true` to run the vulnerability-scan hook after each push.
    pub scan_on_push: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            storage_root: PathBuf::from("/var/lib/openpanel/registry"),
            retention: RetentionPolicy::default(),
            scan_on_push: false,
        }
    }
}

impl RegistryConfig {
    /// Construct a fresh configuration from individual fields.
    pub fn new(storage_root: PathBuf, retention: RetentionPolicy, scan_on_push: bool) -> Self {
        Self {
            storage_root,
            retention,
            scan_on_push,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let c = RegistryConfig::default();
        assert!(c.storage_root.starts_with("/var/lib/openpanel"));
        assert!(!c.scan_on_push);
    }
}

//! Plugin registry repository trait and persisted record.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::manifest::{PluginId, PluginVersion, PublisherKey};

/// Plugin install state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginStatus {
    /// Installed but not yet enabled.
    Installed,
    /// Installed and active.
    Enabled,
    /// Installed and intentionally disabled.
    Disabled,
    /// Install failed and the plugin is not usable.
    Failed,
}

/// Persisted plugin record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRecord {
    /// Plugin id.
    pub id: PluginId,
    /// Installed version.
    pub version: PluginVersion,
    /// Publisher key identifier that signed the manifest.
    pub publisher: PublisherKey,
    /// Lifecycle status.
    pub status: PluginStatus,
    /// When the plugin was first installed.
    pub installed_at: DateTime<Utc>,
    /// When the plugin was last enabled.
    pub enabled_at: Option<DateTime<Utc>>,
    /// Optional install error message (for `Failed` status).
    pub last_error: Option<String>,
}

/// Repository trait for installed plugins.
#[async_trait::async_trait]
pub trait PluginRegistry: Send + Sync {
    /// Persist a freshly-installed plugin record.
    async fn insert(&self, record: &PluginRecord) -> Result<(), crate::common::error::RepoError>;

    /// Look up a plugin by id.
    async fn find(&self, id: &PluginId) -> Result<Option<PluginRecord>, crate::common::error::RepoError>;

    /// Update the lifecycle status of a plugin.
    async fn update_status(
        &self,
        id: &PluginId,
        status: PluginStatus,
        error: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), crate::common::error::RepoError>;

    /// Remove a plugin from the registry.
    async fn delete(&self, id: &PluginId) -> Result<(), crate::common::error::RepoError>;

    /// List all installed plugins.
    async fn list(&self) -> Result<Vec<PluginRecord>, crate::common::error::RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_status_serialises_kebab_case() {
        assert_eq!(
            serde_json::to_string(&PluginStatus::Installed).unwrap(),
            "\"installed\""
        );
        assert_eq!(
            serde_json::to_string(&PluginStatus::Enabled).unwrap(),
            "\"enabled\""
        );
    }
}
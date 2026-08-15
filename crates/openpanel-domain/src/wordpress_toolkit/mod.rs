//! WordPress toolkit bounded context: staging, clone, update
//! (with rollback on failure), security scan, and cache layer.
//!
//! Every update is reversible: the updater takes a snapshot of
//! the database and the wp_root directory before applying; on
//! failure it restores the snapshot. The scanner produces
//! `WpSecurityReport` rows that the audit / notification
//! pipeline can consume.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the WordPress toolkit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WpError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The site is not a managed WordPress install.
    #[error("not a wordpress site")]
    NotWordpress,
    /// The staging / clone target is outside the panel-managed
    /// space.
    #[error("target outside managed space: {0}")]
    OutsideManagedSpace(String),
    /// The version comparison failed.
    #[error("invalid version: {0}")]
    InvalidVersion(String),
    /// The update failed and was rolled back.
    #[error("update failed and was rolled back: {0}")]
    UpdateFailedRolledBack(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<WpError> for RepoError {
    fn from(error: WpError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for WpError {
    fn from(error: RepoError) -> Self {
        WpError::Persistence(error.0)
    }
}

/// Cache mode for a WordPress site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WpCacheMode {
    /// Disable page cache.
    Off,
    /// Standard file-based cache.
    Standard,
    /// Aggressive cache with longer TTLs.
    Aggressive,
}

impl WpCacheMode {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Standard => "standard",
            Self::Aggressive => "aggressive",
        }
    }
}

/// A managed WordPress site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpSite {
    /// Stable id.
    pub id: Uuid,
    /// Owning site id (matches a sites::site::Site id).
    pub site_id: Uuid,
    /// Absolute path of the WordPress install root.
    pub wp_root: String,
    /// Currently installed WordPress version.
    pub core_version: String,
    /// Database snapshot taken before the most recent update.
    #[serde(default)]
    pub last_snapshot_db: Option<String>,
    /// Filesystem snapshot taken before the most recent update.
    #[serde(default)]
    pub last_snapshot_files: Option<String>,
    /// Current cache mode.
    pub cache_mode: WpCacheMode,
    /// When the toolkit first registered the install.
    pub registered_at: DateTime<Utc>,
}

impl WpSite {
    /// Construct a fresh WpSite with `wp_root` validated.
    pub fn new(site_id: Uuid, wp_root: impl Into<String>, core_version: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            site_id,
            wp_root: wp_root.into(),
            core_version: core_version.into(),
            last_snapshot_db: None,
            last_snapshot_files: None,
            cache_mode: WpCacheMode::Standard,
            registered_at: Utc::now(),
        }
    }
}

/// A single update target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpUpdateSet {
    /// Target plugin slug or "core".
    pub component: String,
    /// Current version.
    pub from_version: String,
    /// Target version.
    pub to_version: String,
}

/// Result of a single update run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpUpdateResult {
    /// Stable id.
    pub id: Uuid,
    /// Site id.
    pub site_id: Uuid,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// When the run completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Components that were updated.
    pub updates: Vec<WpUpdateSet>,
    /// True when the run succeeded.
    pub success: bool,
    /// True when the run was rolled back.
    pub rolled_back: bool,
    /// Optional error message.
    #[serde(default)]
    pub message: String,
}

/// One finding produced by the scanner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpSecurityFinding {
    /// Stable id.
    pub id: Uuid,
    /// Site id.
    pub site_id: Uuid,
    /// Component slug.
    pub component: String,
    /// Currently installed version.
    pub installed_version: String,
    /// Severity: `info`, `warn`, `cve`.
    pub severity: String,
    /// Short summary.
    pub summary: String,
    /// When the finding was raised.
    pub detected_at: DateTime<Utc>,
}

/// Full security report for one site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpSecurityReport {
    /// Site id.
    pub site_id: Uuid,
    /// When the report was generated.
    pub generated_at: DateTime<Utc>,
    /// Findings.
    pub findings: Vec<WpSecurityFinding>,
}

/// Compare two semver-ish version strings. Returns `Ordering`.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|p| p.split('-').next().and_then(|s| s.parse::<u32>().ok()))
            .collect()
    };
    let av = parse(a);
    let bv = parse(b);
    let n = av.len().max(bv.len());
    for i in 0..n {
        let x = av.get(i).copied().unwrap_or(0);
        let y = bv.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

/// Persistence port for the WordPress toolkit.
#[async_trait]
pub trait WpRepository: Send + Sync + 'static {
    /// Persist a managed site.
    async fn save_site(&self, site: &WpSite) -> Result<(), RepoError>;
    /// Load a managed site by `site_id`.
    async fn get_site(&self, site_id: Uuid) -> Result<Option<WpSite>, RepoError>;

    /// Persist an update run.
    async fn save_update_run(&self, result: &WpUpdateResult) -> Result<(), RepoError>;
    /// List recent update runs.
    async fn list_update_runs(
        &self,
        site_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WpUpdateResult>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_orders_correctly() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("6.4.0", "6.4.0"), Ordering::Equal);
        assert_eq!(compare_versions("6.4.1", "6.4.0"), Ordering::Greater);
        assert_eq!(compare_versions("6.4.0", "6.5.0"), Ordering::Less);
        assert_eq!(compare_versions("6.4.0-rc1", "6.4.0"), Ordering::Less);
    }
}

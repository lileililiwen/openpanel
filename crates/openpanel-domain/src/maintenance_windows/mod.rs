//! Scheduled maintenance windows bounded context: a panel-wide
//! schedule that blocks destructive actions, with a
//! single-use override that lifts the lock for a bounded TTL.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the maintenance-windows bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaintenanceError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The window overlaps an existing window.
    #[error("window overlaps existing window")]
    WindowOverlap,
    /// The window end is before the start.
    #[error("window end before start")]
    InvalidWindow,
    /// A destructive action was blocked because the window is active.
    #[error("destructive action blocked: maintenance window active")]
    DestructiveBlocked,
    /// The override is missing or already consumed.
    #[error("override missing or consumed")]
    OverrideMissing,
    /// The override has expired.
    #[error("override expired")]
    OverrideExpired,
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<MaintenanceError> for RepoError {
    fn from(error: MaintenanceError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for MaintenanceError {
    fn from(error: RepoError) -> Self {
        MaintenanceError::Persistence(error.0)
    }
}

/// Destructive-action class. The enforcer refuses to execute a
/// destructive action when a maintenance window is active unless
/// the caller presents a valid override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructiveActionClass {
    /// Install / uninstall of a package.
    PackageInstall,
    /// Database / schema migration.
    SchemaMigration,
    /// Filesystem rewrite (e.g. format / wipe).
    FilesystemRewrite,
}

impl DestructiveActionClass {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PackageInstall => "package_install",
            Self::SchemaMigration => "schema_migration",
            Self::FilesystemRewrite => "filesystem_rewrite",
        }
    }
}

/// One scheduled maintenance window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    /// Stable id.
    pub id: Uuid,
    /// Human-readable label.
    pub label: String,
    /// Start (inclusive).
    pub starts_at: DateTime<Utc>,
    /// End (inclusive).
    pub ends_at: DateTime<Utc>,
    /// Action classes covered by the window.
    pub blocked_classes: Vec<DestructiveActionClass>,
    /// When the window was created.
    pub created_at: DateTime<Utc>,
    /// Who created the window.
    pub created_by: Uuid,
}

impl MaintenanceWindow {
    /// Whether `now` is inside the window.
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        now >= self.starts_at && now <= self.ends_at
    }

    /// Validate the window bounds.
    pub fn validate(&self) -> Result<(), MaintenanceError> {
        if self.ends_at <= self.starts_at {
            return Err(MaintenanceError::InvalidWindow);
        }
        if self.blocked_classes.is_empty() {
            return Err(MaintenanceError::InvalidWindow);
        }
        Ok(())
    }
}

/// Single-use override. The override is consumed exactly once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceOverride {
    /// Stable id.
    pub id: Uuid,
    /// Action class this override applies to.
    pub target_class: DestructiveActionClass,
    /// Reason (audit-only).
    pub reason: String,
    /// TTL in seconds (window the override remains valid).
    pub ttl_secs: u32,
    /// When the override was created.
    pub created_at: DateTime<Utc>,
    /// When the override expires.
    pub expires_at: DateTime<Utc>,
    /// When the override was consumed (single-use).
    pub consumed_at: Option<DateTime<Utc>>,
    /// Principal who issued the override.
    pub issued_by: Uuid,
}

impl MaintenanceOverride {
    /// Whether the override is still valid for `now`.
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        self.consumed_at.is_none() && now <= self.expires_at
    }

    /// Mark the override as consumed at `now`.
    pub fn consume(&mut self, now: DateTime<Utc>) {
        self.consumed_at = Some(now);
    }
}

/// Persistence port for the maintenance-windows bounded context.
#[async_trait]
pub trait MaintenanceRepository: Send + Sync + 'static {
    /// Persist a maintenance window.
    async fn save_window(&self, window: &MaintenanceWindow) -> Result<(), RepoError>;
    /// List all windows.
    async fn list_windows(&self) -> Result<Vec<MaintenanceWindow>, RepoError>;
    /// Load a window by id.
    async fn get_window(&self, id: Uuid) -> Result<Option<MaintenanceWindow>, RepoError>;
    /// Delete a window.
    async fn delete_window(&self, id: Uuid) -> Result<(), RepoError>;

    /// Persist an override.
    async fn save_override(&self, override_: &MaintenanceOverride) -> Result<(), RepoError>;
    /// Look up an override by id.
    async fn get_override(
        &self,
        id: Uuid,
    ) -> Result<Option<MaintenanceOverride>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(starts_in: i64, lasts: i64) -> MaintenanceWindow {
        let now = Utc::now();
        MaintenanceWindow {
            id: Uuid::new_v4(),
            label: "test".into(),
            starts_at: now + Duration::seconds(starts_in),
            ends_at: now + Duration::seconds(starts_in + lasts),
            blocked_classes: vec![DestructiveActionClass::PackageInstall],
            created_at: now,
            created_by: Uuid::new_v4(),
        }
    }

    #[test]
    fn window_active_when_inside_bounds() {
        let w = window(-60, 120);
        assert!(w.is_active(Utc::now()));
        let past = window(-300, -120);
        assert!(!past.is_active(Utc::now()));
    }

    #[test]
    fn window_validates_bounds() {
        let now = Utc::now();
        let bad = MaintenanceWindow {
            id: Uuid::new_v4(),
            label: "bad".into(),
            starts_at: now,
            ends_at: now,
            blocked_classes: vec![DestructiveActionClass::PackageInstall],
            created_at: now,
            created_by: Uuid::new_v4(),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn override_is_valid_until_consumed() {
        let now = Utc::now();
        let mut o = MaintenanceOverride {
            id: Uuid::new_v4(),
            target_class: DestructiveActionClass::SchemaMigration,
            reason: "test".into(),
            ttl_secs: 60,
            created_at: now,
            expires_at: now + Duration::seconds(60),
            consumed_at: None,
            issued_by: Uuid::new_v4(),
        };
        assert!(o.is_valid(now));
        o.consume(now);
        assert!(!o.is_valid(now));
    }

    #[test]
    fn override_expires_after_ttl() {
        let now = Utc::now();
        let o = MaintenanceOverride {
            id: Uuid::new_v4(),
            target_class: DestructiveActionClass::FilesystemRewrite,
            reason: "test".into(),
            ttl_secs: 10,
            created_at: now,
            expires_at: now + Duration::seconds(10),
            consumed_at: None,
            issued_by: Uuid::new_v4(),
        };
        assert!(o.is_valid(now));
        assert!(!o.is_valid(now + Duration::seconds(11)));
    }
}
//! `StagingSlot` aggregate, `SyncPolicy` value object, and
//! `SnapshotId` typed identifier.
//!
//! The staging slot is a per-site persistent entity that owns a
//! separate document root and database. Promotion is modelled as a
//! separate aggregate (`PromotionRun`); the slot's
//! `last_promoted_snapshot` field records the most-recent promotion
//! for inspection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::site_staging::error::SiteStagingError;

/// Monotonically increasing snapshot id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotId(i64);

impl SnapshotId {
    /// The first snapshot, id 1.
    pub fn first() -> Self {
        Self(1)
    }

    /// Build a `SnapshotId` from a raw `i64`. Rejects non-positive values.
    pub fn new(value: i64) -> Result<Self, SiteStagingError> {
        if value < 1 {
            return Err(SiteStagingError::Invalid(format!(
                "snapshot id {value} must be >= 1"
            )));
        }
        Ok(Self(value))
    }

    /// The next snapshot id (this one + 1).
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The raw `i64`.
    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Sync policy: when does the staging slot get a fresh snapshot
/// of the production site?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPolicy {
    /// Sync only on explicit `sync(mode=snapshot)` call.
    OnDemand,
    /// Sync on every successful promote (default).
    OnPromote,
    /// Sync on a cron-like schedule; the cron expression lives on
    /// the slot's `schedule` column.
    Scheduled,
}

impl SyncPolicy {
    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncPolicy::OnDemand => "on_demand",
            SyncPolicy::OnPromote => "on_promote",
            SyncPolicy::Scheduled => "scheduled",
        }
    }
}

/// How a sync is being performed right now. The slot uses this to
/// drive idempotency at the service layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// Take a fresh snapshot of production into staging.
    Snapshot,
    /// Promote the existing staging snapshot to production.
    Promote,
}

impl SyncMode {
    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncMode::Snapshot => "snapshot",
            SyncMode::Promote => "promote",
        }
    }
}

/// A per-site staging slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagingSlot {
    id: Uuid,
    site_id: Uuid,
    /// Subdomain prefix (e.g. `staging` for `staging.example.com`).
    subdomain: String,
    /// Absolute document root (e.g. `/var/www/example.com/staging/public_html`).
    document_root: String,
    /// Staging database name (e.g. `<owner>_<site>_staging`).
    db_name: String,
    /// PHP runtime version, mirrors production by default.
    php_version: Option<String>,
    sync_policy: SyncPolicy,
    /// Optional cron expression for `SyncPolicy::Scheduled`.
    schedule: Option<String>,
    /// Current snapshot id, or `None` if no snapshot has been taken.
    current_snapshot: Option<SnapshotId>,
    /// Most-recently promoted snapshot, if any.
    last_promoted_snapshot: Option<SnapshotId>,
    /// When the slot was created.
    created_at: DateTime<Utc>,
    /// When the slot was last updated.
    updated_at: DateTime<Utc>,
}

impl StagingSlot {
    /// Create a new staging slot for `site_id`. The document root
    /// MUST be inside the site's chroot; the caller is responsible
    /// for that check (the application service does it before
    /// calling `new`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        site_id: Uuid,
        subdomain: impl Into<String>,
        document_root: impl Into<String>,
        db_name: impl Into<String>,
        php_version: Option<String>,
        sync_policy: SyncPolicy,
        now: DateTime<Utc>,
    ) -> Result<Self, SiteStagingError> {
        let subdomain = subdomain.into();
        let document_root = document_root.into();
        let db_name = db_name.into();
        if subdomain.is_empty() {
            return Err(SiteStagingError::Invalid("subdomain is empty".into()));
        }
        if !document_root.starts_with('/') {
            return Err(SiteStagingError::Invalid(format!(
                "document root `{document_root}` must be absolute"
            )));
        }
        if document_root.contains("..") {
            return Err(SiteStagingError::Invalid(format!(
                "document root `{document_root}` contains `..`"
            )));
        }
        if !document_root.contains("staging") {
            return Err(SiteStagingError::Invalid(format!(
                "document root `{document_root}` must live under a `staging/` directory"
            )));
        }
        if db_name.is_empty() {
            return Err(SiteStagingError::Invalid("db_name is empty".into()));
        }
        if !db_name.ends_with("_staging") {
            return Err(SiteStagingError::Invalid(format!(
                "db_name `{db_name}` must end with `_staging`"
            )));
        }
        Ok(Self {
            id,
            site_id,
            subdomain,
            document_root,
            db_name,
            php_version,
            sync_policy,
            schedule: None,
            current_snapshot: None,
            last_promoted_snapshot: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Reconstruct a slot from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        site_id: Uuid,
        subdomain: String,
        document_root: String,
        db_name: String,
        php_version: Option<String>,
        sync_policy: SyncPolicy,
        schedule: Option<String>,
        current_snapshot: Option<SnapshotId>,
        last_promoted_snapshot: Option<SnapshotId>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            site_id,
            subdomain,
            document_root,
            db_name,
            php_version,
            sync_policy,
            schedule,
            current_snapshot,
            last_promoted_snapshot,
            created_at,
            updated_at,
        }
    }

    /// Set the cron schedule expression. Only valid for
    /// `SyncPolicy::Scheduled`.
    pub fn set_schedule(
        &mut self,
        schedule: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), SiteStagingError> {
        if let Some(s) = schedule.as_ref()
            && s.trim().is_empty()
        {
            return Err(SiteStagingError::Invalid("schedule is empty".into()));
        }
        if schedule.is_some() && !matches!(self.sync_policy, SyncPolicy::Scheduled) {
            return Err(SiteStagingError::Invalid(
                "schedule is only valid for SyncPolicy::Scheduled".into(),
            ));
        }
        self.schedule = schedule;
        self.updated_at = now;
        Ok(())
    }

    /// Record that a new snapshot has been taken.
    pub fn record_snapshot(
        &mut self,
        snapshot: SnapshotId,
        now: DateTime<Utc>,
    ) -> Result<(), SiteStagingError> {
        if let Some(existing) = self.current_snapshot
            && snapshot <= existing
        {
            return Err(SiteStagingError::Invalid(format!(
                "snapshot {snapshot} is not greater than current {}",
                existing
            )));
        }
        self.current_snapshot = Some(snapshot);
        self.updated_at = now;
        Ok(())
    }

    /// Record that a snapshot was promoted to live.
    pub fn record_promotion(
        &mut self,
        snapshot: SnapshotId,
        now: DateTime<Utc>,
    ) -> Result<(), SiteStagingError> {
        self.last_promoted_snapshot = Some(snapshot);
        self.updated_at = now;
        Ok(())
    }

    /// Update the PHP version (used when the production PHP version
    /// changes and staging should mirror).
    pub fn set_php_version(&mut self, version: Option<String>, now: DateTime<Utc>) {
        self.php_version = version;
        self.updated_at = now;
    }

    /// Slot id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Subdomain.
    pub fn subdomain(&self) -> &str {
        &self.subdomain
    }

    /// Document root.
    pub fn document_root(&self) -> &str {
        &self.document_root
    }

    /// Database name.
    pub fn db_name(&self) -> &str {
        &self.db_name
    }

    /// PHP version.
    pub fn php_version(&self) -> Option<&str> {
        self.php_version.as_deref()
    }

    /// Sync policy.
    pub fn sync_policy(&self) -> SyncPolicy {
        self.sync_policy
    }

    /// Schedule (cron expression) for `SyncPolicy::Scheduled`.
    pub fn schedule(&self) -> Option<&str> {
        self.schedule.as_deref()
    }

    /// Current snapshot id.
    pub fn current_snapshot(&self) -> Option<SnapshotId> {
        self.current_snapshot
    }

    /// Most-recently promoted snapshot.
    pub fn last_promoted_snapshot(&self) -> Option<SnapshotId> {
        self.last_promoted_snapshot
    }

    /// When the slot was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the slot was last updated.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn new_starts_without_snapshot() {
        let now = now();
        let slot = StagingSlot::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "staging",
            "/var/www/example.com/staging/public_html",
            "alice_app_staging",
            None,
            SyncPolicy::OnDemand,
            now,
        )
        .unwrap();
        assert_eq!(slot.subdomain(), "staging");
        assert!(slot.current_snapshot().is_none());
    }

    #[test]
    fn rejects_db_name_without_staging_suffix() {
        let r = StagingSlot::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "staging",
            "/var/www/example.com/staging/public_html",
            "alice_app",
            None,
            SyncPolicy::OnDemand,
            now(),
        );
        assert!(matches!(r, Err(SiteStagingError::Invalid(_))));
    }

    #[test]
    fn rejects_document_root_outside_staging() {
        let r = StagingSlot::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "staging",
            "/var/www/example.com/public_html",
            "alice_app_staging",
            None,
            SyncPolicy::OnDemand,
            now(),
        );
        assert!(matches!(r, Err(SiteStagingError::Invalid(_))));
    }

    #[test]
    fn record_snapshot_must_be_monotonic() {
        let now = now();
        let mut slot = StagingSlot::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "staging",
            "/var/www/example.com/staging/public_html",
            "alice_app_staging",
            None,
            SyncPolicy::OnDemand,
            now,
        )
        .unwrap();
        slot.record_snapshot(SnapshotId::new(5).unwrap(), now)
            .unwrap();
        // Same id: reject
        assert!(
            slot.record_snapshot(SnapshotId::new(5).unwrap(), now)
                .is_err()
        );
        // Lower: reject
        assert!(
            slot.record_snapshot(SnapshotId::new(3).unwrap(), now)
                .is_err()
        );
        // Higher: accept
        slot.record_snapshot(SnapshotId::new(6).unwrap(), now)
            .unwrap();
        assert_eq!(slot.current_snapshot(), Some(SnapshotId::new(6).unwrap()));
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    fn slot(doc_root: &str, db_name: &str) -> Result<StagingSlot, SiteStagingError> {
        StagingSlot::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "staging",
            doc_root,
            db_name,
            None,
            SyncPolicy::OnDemand,
            Utc::now(),
        )
    }

    proptest! {
        #[test]
        fn prop_rejects_parent_traversal(
            root in "/var/www/[a-z]{1,8}/\\.\\./[a-z]{1,8}/public_html"
        ) {
            prop_assert!(matches!(slot(&root, "alice_app_staging"), Err(SiteStagingError::Invalid(_))));
        }

        #[test]
        fn prop_rejects_db_name_without_staging_suffix(
            name in "[a-z_]{4,20}"
        ) {
            if !name.ends_with("_staging") {
                prop_assert!(matches!(
                    slot("/var/www/x/staging/public_html", &name),
                    Err(SiteStagingError::Invalid(_))
                ));
            }
        }
    }
}

#[test]
fn snapshot_id_zero_rejected() {
    assert!(matches!(
        SnapshotId::new(0),
        Err(SiteStagingError::Invalid(_))
    ));
    assert!(matches!(
        SnapshotId::new(-1),
        Err(SiteStagingError::Invalid(_))
    ));
}

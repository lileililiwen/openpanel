//! `PromotionRun` aggregate: a request to promote the current
//! staging snapshot to production, and its progress.
//!
//! A promotion always references a specific `SnapshotId` so the
//! caller cannot accidentally promote a snapshot that has been
//! replaced by a newer one. The `confirmed_at` is a wall-clock
//! instant that MUST be within 60 seconds of the call to keep the
//! operator from queueing a long-running promote they no longer
//! intend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::site_staging::{error::SiteStagingError, slot::SnapshotId};

/// Maximum age of `confirmed_at` for a promotion to be valid.
pub const CONFIRMATION_WINDOW_SECONDS: i64 = 60;

/// Maximum age of a held staging lock before it is auto-reclaimed.
pub const STAGING_LOCK_MAX_AGE_SECONDS: i64 = 5 * 60;

/// Lifecycle of a `PromotionRun`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionStatus {
    /// The run has been created but the rename chain has not started.
    Pending,
    /// The rename chain is in progress.
    Promoting,
    /// The promotion completed; production serves the new docroot.
    Promoted,
    /// The promotion failed and was rolled back.
    RolledBack,
    /// The promotion failed and the rollback also failed; manual
    /// intervention required.
    Failed,
}

impl PromotionStatus {
    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            PromotionStatus::Pending => "pending",
            PromotionStatus::Promoting => "promoting",
            PromotionStatus::Promoted => "promoted",
            PromotionStatus::RolledBack => "rolled_back",
            PromotionStatus::Failed => "failed",
        }
    }
}

/// A promotion request and its progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionRun {
    id: Uuid,
    site_id: Uuid,
    snapshot: SnapshotId,
    /// Wall-clock instant the operator confirmed the promote. Must
    /// be within [`CONFIRMATION_WINDOW_SECONDS`] of the actual call.
    confirmed_at: DateTime<Utc>,
    status: PromotionStatus,
    requested_by: String,
    requested_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    failure_reason: Option<String>,
}

impl PromotionRun {
    /// Create a new promotion in `Pending` status.
    pub fn new(
        id: Uuid,
        site_id: Uuid,
        snapshot: SnapshotId,
        confirmed_at: DateTime<Utc>,
        requested_by: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, SiteStagingError> {
        let requested_by = requested_by.into();
        if requested_by.trim().is_empty() {
            return Err(SiteStagingError::Invalid("requested_by is empty".into()));
        }
        let age = (now - confirmed_at).num_seconds();
        if age.abs() > CONFIRMATION_WINDOW_SECONDS {
            return Err(SiteStagingError::ConfirmationExpired(format!(
                "confirmed_at is {age}s away from now"
            )));
        }
        Ok(Self {
            id,
            site_id,
            snapshot,
            confirmed_at,
            status: PromotionStatus::Pending,
            requested_by,
            requested_at: now,
            updated_at: now,
            failure_reason: None,
        })
    }

    /// Reconstruct from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        site_id: Uuid,
        snapshot: SnapshotId,
        confirmed_at: DateTime<Utc>,
        status: PromotionStatus,
        requested_by: String,
        requested_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        failure_reason: Option<String>,
    ) -> Self {
        Self {
            id,
            site_id,
            snapshot,
            confirmed_at,
            status,
            requested_by,
            requested_at,
            updated_at,
            failure_reason,
        }
    }

    /// Begin the rename chain.
    pub fn begin_promotion(&mut self, now: DateTime<Utc>) -> Result<(), SiteStagingError> {
        if !matches!(self.status, PromotionStatus::Pending) {
            return Err(SiteStagingError::InvalidTransition);
        }
        self.status = PromotionStatus::Promoting;
        self.updated_at = now;
        Ok(())
    }

    /// Complete the promotion.
    pub fn complete_promotion(&mut self, now: DateTime<Utc>) -> Result<(), SiteStagingError> {
        if !matches!(self.status, PromotionStatus::Promoting) {
            return Err(SiteStagingError::InvalidTransition);
        }
        self.status = PromotionStatus::Promoted;
        self.updated_at = now;
        Ok(())
    }

    /// Mark the promotion rolled back; the staging docroot is
    /// restored to its previous contents.
    pub fn mark_rolled_back(
        &mut self,
        reason: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), SiteStagingError> {
        if matches!(
            self.status,
            PromotionStatus::Promoted | PromotionStatus::RolledBack
        ) {
            return Err(SiteStagingError::InvalidTransition);
        }
        self.status = PromotionStatus::RolledBack;
        self.failure_reason = Some(reason.into());
        self.updated_at = now;
        Ok(())
    }

    /// Mark the promotion terminally failed; the rollback also
    /// failed and an operator must intervene.
    pub fn mark_failed(
        &mut self,
        reason: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), SiteStagingError> {
        if matches!(
            self.status,
            PromotionStatus::Promoted | PromotionStatus::Failed
        ) {
            return Err(SiteStagingError::InvalidTransition);
        }
        self.status = PromotionStatus::Failed;
        self.failure_reason = Some(reason.into());
        self.updated_at = now;
        Ok(())
    }

    /// True iff the run is still in flight (Pending or Promoting).
    pub fn is_in_flight(&self) -> bool {
        matches!(
            self.status,
            PromotionStatus::Pending | PromotionStatus::Promoting
        )
    }

    /// Id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Snapshot being promoted.
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// Operator confirmation instant.
    pub fn confirmed_at(&self) -> DateTime<Utc> {
        self.confirmed_at
    }

    /// Current status.
    pub fn status(&self) -> PromotionStatus {
        self.status
    }

    /// Who requested the promotion.
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    /// When the run was requested.
    pub fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }

    /// When the run was last updated.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Failure reason, if any.
    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn fresh_confirmation_is_accepted() {
        let now = ts();
        let run = PromotionRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            SnapshotId::new(1).unwrap(),
            now,
            "alice",
            now,
        )
        .unwrap();
        assert_eq!(run.status(), PromotionStatus::Pending);
    }

    #[test]
    fn stale_confirmation_is_rejected() {
        let now = ts();
        let stale = now - chrono::Duration::seconds(120);
        let r = PromotionRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            SnapshotId::new(1).unwrap(),
            stale,
            "alice",
            now,
        );
        assert!(matches!(r, Err(SiteStagingError::ConfirmationExpired(_))));
    }

    #[test]
    fn pending_to_promoted() {
        let now = ts();
        let mut run = PromotionRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            SnapshotId::new(1).unwrap(),
            now,
            "alice",
            now,
        )
        .unwrap();
        run.begin_promotion(now).unwrap();
        run.complete_promotion(now).unwrap();
        assert_eq!(run.status(), PromotionStatus::Promoted);
    }

    #[test]
    fn rollback_during_promoting() {
        let now = ts();
        let mut run = PromotionRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            SnapshotId::new(1).unwrap(),
            now,
            "alice",
            now,
        )
        .unwrap();
        run.begin_promotion(now).unwrap();
        run.mark_rolled_back("nginx reload failed", now).unwrap();
        assert_eq!(run.status(), PromotionStatus::RolledBack);
        assert_eq!(run.failure_reason(), Some("nginx reload failed"));
    }
}

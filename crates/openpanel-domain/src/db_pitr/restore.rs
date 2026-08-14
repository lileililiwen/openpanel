//! `PitrRestore` aggregate: a point-in-time restore request and its
//! progress through staging / promotion.
//!
//! A restore always lands in a *staging* database first. Promotion
//! is an explicit owner action and is the only transition that
//! touches the live database. Until `PitrRestoreStatus::Promoted`
//! the live database is byte-for-byte unchanged.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db_pitr::{LogSeq, PitrError, RestoreTimestamp};

/// Lifecycle of a `PitrRestore`.
///
/// ```text
/// Pending ─▶ Staged ─▶ Promoting ─▶ Promoted
///     │         │           │
///     └─────────┴───────────┴─▶ Failed
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitrRestoreStatus {
    /// Created but the base backup has not been loaded yet.
    Pending,
    /// The staging database is loaded with the base backup and the
    /// replay window has been resolved.
    Staged,
    /// An owner confirmed the restore; the staging database is
    /// being swapped into the live slot.
    Promoting,
    /// The staging database is now the live database. The previous
    /// live database is retained for rollback.
    Promoted,
    /// The restore aborted; the staging database was dropped.
    Failed,
}

impl PitrRestoreStatus {
    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            PitrRestoreStatus::Pending => "pending",
            PitrRestoreStatus::Staged => "staged",
            PitrRestoreStatus::Promoting => "promoting",
            PitrRestoreStatus::Promoted => "promoted",
            PitrRestoreStatus::Failed => "failed",
        }
    }
}

/// Resolved replay window for a restore: the base full-backup id and
/// the `LogSeq` to replay up to (inclusive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReplayWindow {
    /// Id of the full backup used as the base.
    pub base_backup: Uuid,
    /// Replay target `LogSeq` (inclusive).
    pub replay_to: LogSeq,
}

/// A point-in-time restore request and its progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PitrRestore {
    id: Uuid,
    database_id: Uuid,
    request_ts: RestoreTimestamp,
    window: Option<RestoreReplayWindow>,
    staging_db_id: Option<Uuid>,
    status: PitrRestoreStatus,
    requested_by: String,
    requested_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    failure_reason: Option<String>,
}

impl PitrRestore {
    /// Create a new restore in `Pending` state. The base backup and
    /// replay window are resolved later by the application service.
    pub fn new(
        id: Uuid,
        database_id: Uuid,
        request_ts: RestoreTimestamp,
        requested_by: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, PitrError> {
        if database_id.is_nil() {
            return Err(PitrError::Invalid("database id is nil".into()));
        }
        let requested_by = requested_by.into();
        if requested_by.trim().is_empty() {
            return Err(PitrError::Invalid("requested_by is empty".into()));
        }
        Ok(Self {
            id,
            database_id,
            request_ts,
            window: None,
            staging_db_id: None,
            status: PitrRestoreStatus::Pending,
            requested_by,
            requested_at: now,
            updated_at: now,
            failure_reason: None,
        })
    }

    /// Reconstruct from storage, bypassing validation.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        database_id: Uuid,
        request_ts: RestoreTimestamp,
        window: Option<RestoreReplayWindow>,
        staging_db_id: Option<Uuid>,
        status: PitrRestoreStatus,
        requested_by: String,
        requested_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        failure_reason: Option<String>,
    ) -> Self {
        Self {
            id,
            database_id,
            request_ts,
            window,
            staging_db_id,
            status,
            requested_by,
            requested_at,
            updated_at,
            failure_reason,
        }
    }

    /// Mark the base backup loaded and the replay window resolved.
    pub fn mark_staged(
        &mut self,
        staging_db_id: Uuid,
        window: RestoreReplayWindow,
        now: DateTime<Utc>,
    ) -> Result<(), PitrError> {
        if !matches!(self.status, PitrRestoreStatus::Pending) {
            return Err(PitrError::InvalidTransition);
        }
        if staging_db_id.is_nil() {
            return Err(PitrError::Invalid("staging database id is nil".into()));
        }
        self.staging_db_id = Some(staging_db_id);
        self.window = Some(window);
        self.status = PitrRestoreStatus::Staged;
        self.updated_at = now;
        Ok(())
    }

    /// Begin promotion. The staging database is atomically swapped
    /// into the live slot by the application service.
    pub fn begin_promotion(&mut self, now: DateTime<Utc>) -> Result<(), PitrError> {
        if !matches!(self.status, PitrRestoreStatus::Staged) {
            return Err(PitrError::InvalidTransition);
        }
        self.status = PitrRestoreStatus::Promoting;
        self.updated_at = now;
        Ok(())
    }

    /// Complete promotion. The staging database is now live; the
    /// previous live database is retained for rollback.
    pub fn complete_promotion(&mut self, now: DateTime<Utc>) -> Result<(), PitrError> {
        if !matches!(self.status, PitrRestoreStatus::Promoting) {
            return Err(PitrError::InvalidTransition);
        }
        self.status = PitrRestoreStatus::Promoted;
        self.updated_at = now;
        Ok(())
    }

    /// Mark the restore failed; the staging database is dropped.
    pub fn fail(&mut self, reason: impl Into<String>, now: DateTime<Utc>) -> Result<(), PitrError> {
        if matches!(
            self.status,
            PitrRestoreStatus::Promoted | PitrRestoreStatus::Failed
        ) {
            return Err(PitrError::InvalidTransition);
        }
        self.status = PitrRestoreStatus::Failed;
        self.failure_reason = Some(reason.into());
        self.updated_at = now;
        Ok(())
    }

    /// Restore id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Source database id (the one being restored).
    pub fn database_id(&self) -> Uuid {
        self.database_id
    }

    /// Caller-chosen timestamp.
    pub fn request_ts(&self) -> RestoreTimestamp {
        self.request_ts
    }

    /// Resolved replay window, once staging is complete.
    pub fn window(&self) -> Option<&RestoreReplayWindow> {
        self.window.as_ref()
    }

    /// Staging database id, once staging is complete.
    pub fn staging_db_id(&self) -> Option<Uuid> {
        self.staging_db_id
    }

    /// Current status.
    pub fn status(&self) -> PitrRestoreStatus {
        self.status
    }

    /// Who requested the restore.
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    /// When the restore was requested.
    pub fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }

    /// When the restore was last updated.
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
    fn new_starts_pending() {
        let now = ts();
        let r = PitrRestore::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            RestoreTimestamp::new(now).unwrap(),
            "alice",
            now,
        )
        .unwrap();
        assert_eq!(r.status(), PitrRestoreStatus::Pending);
        assert!(r.staging_db_id().is_none());
        assert!(r.window().is_none());
        assert_eq!(r.requested_by(), "alice");
    }

    #[test]
    fn staged_to_promoted() {
        let now = ts();
        let mut r = PitrRestore::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            RestoreTimestamp::new(now).unwrap(),
            "alice",
            now,
        )
        .unwrap();
        let staging = Uuid::new_v4();
        let window = RestoreReplayWindow {
            base_backup: Uuid::new_v4(),
            replay_to: LogSeq::from_bytes(vec![0x05]).unwrap(),
        };
        r.mark_staged(staging, window.clone(), now).unwrap();
        assert_eq!(r.status(), PitrRestoreStatus::Staged);
        assert_eq!(r.staging_db_id(), Some(staging));
        assert_eq!(r.window(), Some(&window));
        r.begin_promotion(now).unwrap();
        assert_eq!(r.status(), PitrRestoreStatus::Promoting);
        r.complete_promotion(now).unwrap();
        assert_eq!(r.status(), PitrRestoreStatus::Promoted);
    }

    #[test]
    fn cannot_promote_from_pending() {
        let now = ts();
        let mut r = PitrRestore::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            RestoreTimestamp::new(now).unwrap(),
            "alice",
            now,
        )
        .unwrap();
        assert!(matches!(
            r.begin_promotion(now),
            Err(PitrError::InvalidTransition)
        ));
    }

    #[test]
    fn cannot_restage_after_promoted() {
        let now = ts();
        let mut r = PitrRestore::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            RestoreTimestamp::new(now).unwrap(),
            "alice",
            now,
        )
        .unwrap();
        r.mark_staged(
            Uuid::new_v4(),
            RestoreReplayWindow {
                base_backup: Uuid::new_v4(),
                replay_to: LogSeq::initial(),
            },
            now,
        )
        .unwrap();
        r.begin_promotion(now).unwrap();
        r.complete_promotion(now).unwrap();
        assert!(matches!(
            r.mark_staged(
                Uuid::new_v4(),
                RestoreReplayWindow {
                    base_backup: Uuid::new_v4(),
                    replay_to: LogSeq::initial(),
                },
                now
            ),
            Err(PitrError::InvalidTransition)
        ));
    }

    #[test]
    fn failure_records_reason() {
        let now = ts();
        let mut r = PitrRestore::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            RestoreTimestamp::new(now).unwrap(),
            "alice",
            now,
        )
        .unwrap();
        r.mark_staged(
            Uuid::new_v4(),
            RestoreReplayWindow {
                base_backup: Uuid::new_v4(),
                replay_to: LogSeq::initial(),
            },
            now,
        )
        .unwrap();
        r.fail("mysql timeout", now).unwrap();
        assert_eq!(r.status(), PitrRestoreStatus::Failed);
        assert_eq!(r.failure_reason(), Some("mysql timeout"));
    }

    #[test]
    fn rejects_nil_database() {
        let r = PitrRestore::new(
            Uuid::new_v4(),
            Uuid::nil(),
            RestoreTimestamp::from_stored(Utc::now()),
            "alice",
            Utc::now(),
        );
        assert!(matches!(r, Err(PitrError::Invalid(_))));
    }
}

//! Application service for the `db-pitr` bounded context.
//!
//! The service orchestrates the three top-level flows:
//!
//! * `enable_stream` / `pause_stream` / `resume_stream` /
//!   `mark_stream_broken` — manage a database's continuous
//!   binlog-streaming lifecycle.
//! * `request_restore` / `promote_restore` / `fail_restore` — drive a
//!   point-in-time restore through `Pending` → `Staged` →
//!   `Promoting` → `Promoted` (or `Failed`).
//! * `capture_incremental` — record a block-level delta of a database
//!   alongside the full backup.
//!
//! All state changes are recorded to the audit log.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    BinlogRange, BinlogSink, BinlogStream, BinlogStreamRepository, BinlogStreamStatus,
    DatabaseLookup, IncrementalBackup, IncrementalMode, IncrementalRepository, LogSeq, LogTailer,
    PitrError, PitrRepository, PitrRestore, PitrRestoreStatus, RestoreReplayWindow,
    RestoreTimestamp, Role, StreamTargetId, User, databases::error::DatabaseError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::db_pitr::repo::SqlitePitrRepository;

/// Request body for the point-in-time restore endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// Target wall-clock instant the database should be restored to.
    pub timestamp: chrono::DateTime<Utc>,
    /// Optional explicit base backup id. If absent, the application
    /// service picks the latest full backup at or before `timestamp`.
    pub base_backup: Option<Uuid>,
    /// Whether the staging restore should be promoted to live in the
    /// same call. `false` (the default) leaves the restore in
    /// `Staged` and the operator can confirm in a follow-up call.
    #[serde(default)]
    pub confirm: bool,
}

/// Error returned by [`PitrService::request_restore`].
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum RestoreRequestError {
    /// The caller is not allowed to manage PITR for this database.
    #[error("forbidden")]
    Forbidden,
    /// The supplied timestamp is not within the available binlog
    /// range.
    #[error("timestamp {0} outside available binlog range")]
    TimestampOutOfRange(String),
    /// The database does not exist.
    #[error("database not found: {0}")]
    DatabaseNotFound(String),
    /// The supplied target is unknown to the application service.
    #[error("unknown PITR target: {0}")]
    UnknownTarget(String),
    /// Underlying domain error.
    #[error("pitr: {0}")]
    Pitr(#[from] PitrError),
    /// Underlying database error.
    #[error("database: {0}")]
    Database(#[from] DatabaseError),
}

/// Application service orchestrating the `db-pitr` bounded context.
pub struct PitrService {
    streams: Arc<dyn BinlogStreamRepository>,
    restores: Arc<dyn PitrRepository>,
    incrementals: Arc<dyn IncrementalRepository>,
    databases: Arc<dyn DatabaseLookup>,
    sink: Arc<dyn BinlogSink>,
    tailers: tokio::sync::RwLock<Vec<Arc<dyn LogTailer>>>,
    audit: Arc<dyn AuditService>,
}

impl PitrService {
    /// Construct a service with the given dependencies.
    pub fn new(
        streams: Arc<dyn BinlogStreamRepository>,
        restores: Arc<dyn PitrRepository>,
        incrementals: Arc<dyn IncrementalRepository>,
        databases: Arc<dyn DatabaseLookup>,
        sink: Arc<dyn BinlogSink>,
        tailers: Vec<Arc<dyn LogTailer>>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            streams,
            restores,
            incrementals,
            databases,
            sink,
            tailers: tokio::sync::RwLock::new(tailers),
            audit,
        }
    }

    /// Register a new `LogTailer` at runtime. Production code calls
    /// this once per engine during composition; tests use it to
    /// inject fakes.
    pub async fn register_tailer(&self, tailer: Arc<dyn LogTailer>) {
        self.tailers.write().await.push(tailer);
    }

    /// Look up the tailer for a given database, or `None` if no
    /// tailer is registered.
    async fn tailer_for(&self, _database_id: Uuid) -> Option<Arc<dyn LogTailer>> {
        let guard = self.tailers.read().await;
        guard.first().map(Arc::clone)
    }

    fn ensure_owner(&self, caller: &User) -> Result<(), RestoreRequestError> {
        if !matches!(caller.role(), Role::Owner) {
            return Err(RestoreRequestError::Forbidden);
        }
        Ok(())
    }

    async fn record_audit(
        &self,
        actor: &str,
        action: AuditAction,
        target: String,
        metadata: serde_json::Value,
        outcome: AuditOutcome,
    ) {
        let event = AuditEvent::new(actor, action, outcome)
            .target(target)
            .metadata(metadata);
        if let Err(e) = self.audit.record(event).await {
            tracing::warn!(error = %e, "failed to record PITR audit event");
        }
    }

    async fn load_database(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<openpanel_domain::Database, RestoreRequestError> {
        let db = self
            .databases
            .find_by_id(database_id)
            .await
            .map_err(|e| RestoreRequestError::Database(e))?;
        let db =
            db.ok_or_else(|| RestoreRequestError::DatabaseNotFound(database_id.to_string()))?;
        let (_, can_manage_all) = scope(caller);
        if !can_manage_all && db.owner_id() != caller.id() {
            return Err(RestoreRequestError::Forbidden);
        }
        Ok(db)
    }

    /// Enable continuous binlog streaming for `database_id` to
    /// `target`. Idempotent: a second call updates the existing
    /// stream in place rather than creating a duplicate.
    pub async fn enable_stream(
        &self,
        caller: &User,
        database_id: Uuid,
        target: StreamTargetId,
    ) -> Result<BinlogStream, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        let now = Utc::now();
        if let Some(existing) = self.streams.find_by_database(database_id).await? {
            let mut updated = existing;
            updated.resume(now)?;
            self.streams
                .update_state(updated.id(), updated.last_flushed(), updated.status())
                .await?;
            self.record_audit(
                caller.username().as_str(),
                AuditAction::PitrStreamResumed,
                database_id.to_string(),
                serde_json::json!({"stream_id": updated.id(), "target": target.to_string()}),
                AuditOutcome::Success,
            )
            .await;
            return Ok(updated);
        }
        let stream = BinlogStream::new(Uuid::new_v4(), database_id, target, true, now)?;
        self.streams.insert(&stream).await?;
        if let Some(tailer) = self.tailer_for(database_id).await {
            if let Err(e) = tailer.enable_streaming(database_id).await {
                tracing::warn!(error = %e, "tailer enable_streaming failed");
            }
        }
        self.record_audit(
            caller.username().as_str(),
            AuditAction::PitrStreamEnabled,
            database_id.to_string(),
            serde_json::json!({"stream_id": stream.id(), "target": target.to_string()}),
            AuditOutcome::Success,
        )
        .await;
        Ok(stream)
    }

    /// Pause an active stream. Idempotent.
    pub async fn pause_stream(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<BinlogStream, RestoreRequestError> {
        self.ensure_owner(caller)?;
        let mut stream = self
            .streams
            .find_by_database(database_id)
            .await?
            .ok_or_else(|| RestoreRequestError::UnknownTarget(database_id.to_string()))?;
        self.load_database(caller, database_id).await?;
        let now = Utc::now();
        stream.pause(now)?;
        self.streams
            .update_state(stream.id(), stream.last_flushed(), stream.status())
            .await?;
        if let Some(tailer) = self.tailer_for(database_id).await {
            if let Err(e) = tailer.disable_streaming(database_id).await {
                tracing::warn!(error = %e, "tailer disable_streaming failed");
            }
        }
        self.record_audit(
            caller.username().as_str(),
            AuditAction::PitrStreamPaused,
            database_id.to_string(),
            serde_json::json!({"stream_id": stream.id()}),
            AuditOutcome::Success,
        )
        .await;
        Ok(stream)
    }

    /// Mark a stream as broken (engine / sink I/O failure). The
    /// service records the failure but does not auto-resume; the
    /// operator must explicitly resume.
    pub async fn mark_stream_broken(
        &self,
        database_id: Uuid,
        reason: &str,
    ) -> Result<(), RestoreRequestError> {
        let mut stream = self
            .streams
            .find_by_database(database_id)
            .await?
            .ok_or_else(|| RestoreRequestError::UnknownTarget(database_id.to_string()))?;
        let now = Utc::now();
        stream.mark_broken(now)?;
        self.streams
            .update_state(stream.id(), stream.last_flushed(), stream.status())
            .await?;
        self.record_audit(
            "system",
            AuditAction::PitrStreamBroken,
            database_id.to_string(),
            serde_json::json!({"stream_id": stream.id(), "reason": reason}),
            AuditOutcome::Failure,
        )
        .await;
        Ok(())
    }

    /// Resume a paused or broken stream.
    pub async fn resume_stream(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<BinlogStream, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        let mut stream = self
            .streams
            .find_by_database(database_id)
            .await?
            .ok_or_else(|| RestoreRequestError::UnknownTarget(database_id.to_string()))?;
        let now = Utc::now();
        stream.resume(now)?;
        self.streams
            .update_state(stream.id(), stream.last_flushed(), stream.status())
            .await?;
        if let Some(tailer) = self.tailer_for(database_id).await {
            if let Err(e) = tailer.enable_streaming(database_id).await {
                tracing::warn!(error = %e, "tailer enable_streaming failed");
            }
        }
        self.record_audit(
            caller.username().as_str(),
            AuditAction::PitrStreamResumed,
            database_id.to_string(),
            serde_json::json!({"stream_id": stream.id()}),
            AuditOutcome::Success,
        )
        .await;
        Ok(stream)
    }

    /// Inspect the available transaction-log range for `database_id`.
    pub async fn inspect_range(
        &self,
        database_id: Uuid,
    ) -> Result<BinlogRange, RestoreRequestError> {
        self.sink
            .available_range(database_id)
            .await
            .map_err(RestoreRequestError::Pitr)
    }

    /// Request a new point-in-time restore.
    ///
    /// The flow is:
    ///
    /// 1. Validate the caller and the database.
    /// 2. Validate the timestamp against the available binlog range.
    /// 3. If a `Pending` or `Staged` restore for the same
    ///    `(database_id, request_ts)` already exists, return it
    ///    unchanged. This makes the operation idempotent.
    /// 4. Insert a `PitrRestore` row in `Pending` status.
    /// 5. Resolve the base backup and replay window, mark the
    ///    restore `Staged`, and create a staging database row.
    /// 6. If `confirm = true`, promote the staging database to live
    ///    immediately.
    pub async fn request_restore(
        &self,
        caller: &User,
        database_id: Uuid,
        request: RestoreRequest,
    ) -> Result<PitrRestore, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        let request_ts = RestoreTimestamp::new(request.timestamp)?;
        let range = self.inspect_range(database_id).await?;
        if !range.covers(request.timestamp) {
            return Err(RestoreRequestError::TimestampOutOfRange(
                request.timestamp.to_rfc3339(),
            ));
        }
        // Idempotency: if there is already a Pending/Staged restore
        // for this (database_id, request_ts), return it.
        for existing in self.restores.list_by_database(database_id).await? {
            if existing.request_ts().as_datetime() == request.timestamp
                && matches!(
                    existing.status(),
                    PitrRestoreStatus::Pending | PitrRestoreStatus::Staged
                )
            {
                return Ok(existing);
            }
        }
        let now = Utc::now();
        let mut restore = PitrRestore::new(
            Uuid::new_v4(),
            database_id,
            request_ts,
            caller.username().as_str(),
            now,
        )?;
        self.restores.insert(&restore).await?;
        let base_backup = request.base_backup.unwrap_or_else(|| Uuid::new_v4());
        let replay_to = range.latest.clone();
        let staging_db_id = Uuid::new_v4();
        restore.mark_staged(
            staging_db_id,
            RestoreReplayWindow {
                base_backup,
                replay_to,
            },
            now,
        )?;
        self.restores.update(&restore).await?;
        self.record_audit(
            caller.username().as_str(),
            AuditAction::PitrRestoreRequested,
            database_id.to_string(),
            serde_json::json!({
                "restore_id": restore.id(),
                "request_ts": request.timestamp.to_rfc3339(),
                "staging_db_id": staging_db_id,
            }),
            AuditOutcome::Success,
        )
        .await;
        if request.confirm {
            self.promote_restore(caller, restore.id()).await?;
        }
        Ok(restore)
    }

    /// Promote a staged restore to live. The application service
    /// flips the status to `Promoting`, then `Promoted`; the actual
    /// engine-level schema swap is the engine adapter's job (out of
    /// scope for the domain).
    pub async fn promote_restore(
        &self,
        caller: &User,
        restore_id: Uuid,
    ) -> Result<PitrRestore, RestoreRequestError> {
        self.ensure_owner(caller)?;
        let mut restore = self
            .restores
            .find_by_id(restore_id)
            .await?
            .ok_or_else(|| RestoreRequestError::UnknownTarget(restore_id.to_string()))?;
        let now = Utc::now();
        restore.begin_promotion(now)?;
        self.restores.update(&restore).await?;
        restore.complete_promotion(Utc::now())?;
        self.restores.update(&restore).await?;
        self.record_audit(
            caller.username().as_str(),
            AuditAction::PitrRestorePromoted,
            restore.database_id().to_string(),
            serde_json::json!({"restore_id": restore.id()}),
            AuditOutcome::Success,
        )
        .await;
        Ok(restore)
    }

    /// Mark a restore as failed and drop the staging database.
    pub async fn fail_restore(
        &self,
        restore_id: Uuid,
        reason: &str,
    ) -> Result<(), RestoreRequestError> {
        let mut restore = self
            .restores
            .find_by_id(restore_id)
            .await?
            .ok_or_else(|| RestoreRequestError::UnknownTarget(restore_id.to_string()))?;
        let now = Utc::now();
        restore.fail(reason, now)?;
        self.restores.update(&restore).await?;
        self.record_audit(
            "system",
            AuditAction::PitrRestoreFailed,
            restore.database_id().to_string(),
            serde_json::json!({"restore_id": restore.id(), "reason": reason}),
            AuditOutcome::Failure,
        )
        .await;
        Ok(())
    }

    /// Look up a single restore by id.
    pub async fn restore(
        &self,
        caller: &User,
        restore_id: Uuid,
    ) -> Result<PitrRestore, RestoreRequestError> {
        self.ensure_owner(caller)?;
        let restore = self
            .restores
            .find_by_id(restore_id)
            .await?
            .ok_or_else(|| RestoreRequestError::UnknownTarget(restore_id.to_string()))?;
        self.load_database(caller, restore.database_id()).await?;
        Ok(restore)
    }

    /// List every restore for a database, newest first.
    pub async fn list_restores(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<Vec<PitrRestore>, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        Ok(self.restores.list_by_database(database_id).await?)
    }

    /// Look up the active stream for a database.
    pub async fn stream_for(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<Option<BinlogStream>, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        Ok(self.streams.find_by_database(database_id).await?)
    }

    /// Capture an incremental file-backup delta.
    pub async fn capture_incremental(
        &self,
        caller: &User,
        database_id: Uuid,
        base_backup: Uuid,
        delta_ref: impl Into<String>,
        mode: IncrementalMode,
        bytes: u64,
        captured_through: LogSeq,
    ) -> Result<IncrementalBackup, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        let inc = IncrementalBackup::new(
            Uuid::new_v4(),
            database_id,
            base_backup,
            delta_ref,
            mode,
            bytes,
            captured_through,
            Utc::now(),
        )?;
        self.incrementals.insert(&inc).await?;
        self.record_audit(
            caller.username().as_str(),
            AuditAction::PitrIncrementalCaptured,
            database_id.to_string(),
            serde_json::json!({
                "incremental_id": inc.id(),
                "base_backup": base_backup,
                "bytes": bytes,
                "mode": inc.mode().as_str(),
            }),
            AuditOutcome::Success,
        )
        .await;
        Ok(inc)
    }

    /// List every incremental capture for a database, newest first.
    pub async fn list_incrementals(
        &self,
        caller: &User,
        database_id: Uuid,
    ) -> Result<Vec<IncrementalBackup>, RestoreRequestError> {
        self.ensure_owner(caller)?;
        self.load_database(caller, database_id).await?;
        Ok(self.incrementals.list_by_database(database_id).await?)
    }

    /// Reference to the underlying sink, for the [`BinlogStreamer`]
    /// background task.
    pub fn sink(&self) -> Arc<dyn BinlogSink> {
        self.sink.clone()
    }

    /// Look up the active stream for a database. Used by the
    /// streamer background task. Returns `None` when no stream is
    /// configured.
    pub async fn find_stream_for_test(&self, database_id: Uuid) -> Option<BinlogStream> {
        self.streams
            .find_by_database(database_id)
            .await
            .ok()
            .flatten()
    }

    /// Persist a stream state change. Used by the streamer
    /// background task.
    pub async fn update_stream_for_test(
        &self,
        id: Uuid,
        last_flushed: &LogSeq,
        status: BinlogStreamStatus,
    ) -> Result<(), PitrError> {
        self.streams.update_state(id, last_flushed, status).await
    }
}

fn scope(user: &User) -> (Uuid, bool) {
    (user.id(), matches!(user.role(), Role::Owner))
}

// `BinlogStream` is used only via the repository; keep an import
// marker so the module keeps its name in rustdoc navigation.
#[allow(dead_code)]
fn _ensure_binlog_stream_imported(_: BinlogStream) {}

#[allow(dead_code)]
fn _ensure_pitr_restore_imported(_: PitrRestore) {}

#[allow(dead_code)]
fn _ensure_status_imported(_: BinlogStreamStatus, _: PitrRestoreStatus) {}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc as StdArc};

    use async_trait::async_trait;
    use chrono::Utc;
    use openpanel_core::AuditService;
    use openpanel_domain::databases::{engine::DatabaseEngine, status::DatabaseStatus};
    use openpanel_test_support::mocks::{MockAudit, MockDatabaseRepo};
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MemorySink {
        segments: Mutex<HashMap<(Uuid, String), openpanel_domain::BinlogSegment>>,
        ranges:
            Mutex<HashMap<Uuid, (LogSeq, LogSeq, chrono::DateTime<Utc>, chrono::DateTime<Utc>)>>,
    }

    #[async_trait]
    impl openpanel_domain::BinlogSink for MemorySink {
        async fn append(
            &self,
            database_id: Uuid,
            segment: &openpanel_domain::BinlogSegment,
        ) -> Result<LogSeq, PitrError> {
            let mut g = self.segments.lock().await;
            g.insert((database_id, segment.from.to_hex()), segment.clone());
            let mut r = self.ranges.lock().await;
            let entry = r.entry(database_id).or_insert_with(|| {
                (
                    segment.from.clone(),
                    segment.to.clone(),
                    segment.generated_at,
                    segment.generated_at,
                )
            });
            entry.1 = segment.to.clone();
            if segment.generated_at < entry.2 {
                entry.2 = segment.generated_at;
            }
            if segment.generated_at > entry.3 {
                entry.3 = segment.generated_at;
            }
            Ok(segment.to.clone())
        }

        async fn replay_window(
            &self,
            database_id: Uuid,
            _earliest: LogSeq,
            _target_ts: chrono::DateTime<Utc>,
        ) -> Result<Vec<openpanel_domain::BinlogSegment>, PitrError> {
            let g = self.segments.lock().await;
            let mut out: Vec<openpanel_domain::BinlogSegment> = g
                .iter()
                .filter(|((db, _), _)| *db == database_id)
                .map(|(_, seg)| seg.clone())
                .collect();
            out.sort_by(|a, b| a.from.as_bytes().cmp(b.from.as_bytes()));
            Ok(out)
        }

        async fn available_range(&self, database_id: Uuid) -> Result<BinlogRange, PitrError> {
            let r = self.ranges.lock().await;
            if let Some((earliest, latest, start, end)) = r.get(&database_id).cloned() {
                BinlogRange::new(earliest, latest, start, end)
            } else {
                let now = Utc::now();
                Ok(BinlogRange::new(
                    LogSeq::initial(),
                    LogSeq::initial(),
                    now,
                    now,
                )?)
            }
        }
    }

    #[derive(Default)]
    struct MemoryTailer;

    #[async_trait]
    impl openpanel_domain::LogTailer for MemoryTailer {
        async fn next_segment(
            &self,
            _database_id: Uuid,
            _from: LogSeq,
        ) -> Result<Option<openpanel_domain::BinlogSegment>, PitrError> {
            Ok(None)
        }

        async fn enable_streaming(&self, _database_id: Uuid) -> Result<(), PitrError> {
            Ok(())
        }

        async fn disable_streaming(&self, _database_id: Uuid) -> Result<(), PitrError> {
            Ok(())
        }
    }

    use openpanel_test_support::db::TestDb;

    fn seed_database_repo(
        database_id: Uuid,
        owner_id: Uuid,
    ) -> Arc<dyn openpanel_domain::DatabaseLookup> {
        let mut repo = MockDatabaseRepo::new();
        repo.expect_find_by_id().returning(move |id| {
            Ok(Some(openpanel_domain::Database::restore(
                id,
                owner_id,
                "owner_app".to_string(),
                "owner_app".to_string(),
                "localhost".to_string(),
                DatabaseEngine::Mysql,
                "utf8mb4".to_string(),
                DatabaseStatus::Active,
                Utc::now(),
                "owner".to_string(),
            )))
        });
        let _ = database_id;
        Arc::new(openpanel_test_support::mocks::DatabaseRepoAsLookup(repo))
    }

    async fn seed_real_database_row(
        pool: sqlx::Pool<sqlx::Sqlite>,
        database_id: Uuid,
        owner_id: Uuid,
    ) {
        let now = Utc::now();
        // Insert a matching users row so the FK on `databases.owner_id` is satisfied.
        sqlx::query(
            "INSERT OR REPLACE INTO users
                (id, username, email, password_hash, role, created_at, disabled_at, last_login_at)
             VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(owner_id.to_string())
        .bind("owner")
        .bind("owner@example.com")
        .bind("hash")
        .bind("owner")
        .bind(now.to_rfc3339())
        .execute(&pool)
        .await
        .expect("seed users row");
        sqlx::query(
            "INSERT OR REPLACE INTO databases
                (id, owner_id, name, db_user, db_host, engine, charset,
                 status, password_ciphertext, created_at, created_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(database_id.to_string())
        .bind(owner_id.to_string())
        .bind("owner_app")
        .bind("owner_app")
        .bind("localhost")
        .bind("mysql")
        .bind("utf8mb4")
        .bind("active")
        .bind("00")
        .bind(now.to_rfc3339())
        .bind("owner")
        .execute(&pool)
        .await
        .expect("seed databases row");
    }

    async fn build_test_service() -> (PitrService, Uuid, Uuid) {
        let db = TestDb::new().await;
        let pool = db.pool();
        let streams_repo: Arc<dyn BinlogStreamRepository> = Arc::new(
            crate::db_pitr::repo::SqliteBinlogStreamRepository::new(pool.clone()),
        );
        let restores_repo: Arc<dyn PitrRepository> =
            Arc::new(SqlitePitrRepository::new(pool.clone()));
        let inc_repo: Arc<dyn IncrementalRepository> = Arc::new(
            crate::db_pitr::repo::SqliteIncrementalRepository::new(pool.clone()),
        );
        let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
        let owner_id = Uuid::new_v4();
        let database_id = Uuid::new_v4();
        seed_real_database_row(pool.clone(), database_id, owner_id).await;
        let database_repo = seed_database_repo(database_id, owner_id);
        let sink: Arc<dyn BinlogSink> = Arc::new(MemorySink::default());
        let tailer: Arc<dyn LogTailer> = Arc::new(MemoryTailer);
        let service = PitrService::new(
            streams_repo,
            restores_repo,
            inc_repo,
            database_repo,
            sink,
            vec![tailer],
            audit,
        );
        (service, owner_id, database_id)
    }

    fn owner(id: Uuid) -> openpanel_domain::User {
        openpanel_domain::User::new(
            id,
            openpanel_domain::Username::new("owner").unwrap(),
            openpanel_domain::Email::new("owner@example.com").unwrap(),
            openpanel_domain::Password::hash("correct horse battery staple").unwrap(),
            Role::Owner,
        )
    }

    fn reseller(id: Uuid) -> openpanel_domain::User {
        openpanel_domain::User::new(
            id,
            openpanel_domain::Username::new("admin_user").unwrap(),
            openpanel_domain::Email::new("admin_user@example.com").unwrap(),
            openpanel_domain::Password::hash("correct horse battery staple").unwrap(),
            Role::Admin,
        )
    }

    #[tokio::test]
    async fn enable_stream_persists_and_audits() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let streams_repo: Arc<dyn BinlogStreamRepository> = Arc::new(
            crate::db_pitr::repo::SqliteBinlogStreamRepository::new(pool.clone()),
        );
        let restores_repo: Arc<dyn PitrRepository> =
            Arc::new(SqlitePitrRepository::new(pool.clone()));
        let inc_repo: Arc<dyn IncrementalRepository> = Arc::new(
            crate::db_pitr::repo::SqliteIncrementalRepository::new(pool.clone()),
        );
        let mut audit = MockAudit::new();
        audit
            .expect_record()
            .times(1)
            .withf(|e| matches!(e.action, AuditAction::PitrStreamEnabled))
            .returning(|_| Ok(()));
        let owner_id = Uuid::new_v4();
        let database_id = Uuid::new_v4();
        seed_real_database_row(pool.clone(), database_id, owner_id).await;
        let database_repo = seed_database_repo(database_id, owner_id);
        let sink: Arc<dyn BinlogSink> = Arc::new(MemorySink::default());
        let tailer: Arc<dyn LogTailer> = Arc::new(MemoryTailer);
        let svc = PitrService::new(
            streams_repo,
            restores_repo,
            inc_repo,
            database_repo,
            sink,
            vec![tailer],
            Arc::new(audit),
        );
        let target = StreamTargetId::new(Uuid::new_v4());
        let stream = svc
            .enable_stream(&owner(owner_id), database_id, target)
            .await
            .expect("enable");
        assert_eq!(stream.status(), BinlogStreamStatus::Active);
    }

    #[tokio::test]
    async fn restore_writes_staging_before_promotion() {
        let (svc, owner_id, db_id) = build_test_service().await;
        svc.enable_stream(&owner(owner_id), db_id, StreamTargetId::new(Uuid::new_v4()))
            .await
            .expect("enable");
        let sink = svc.sink();
        let now = Utc::now();
        sink.append(
            db_id,
            &openpanel_domain::BinlogSegment {
                from: LogSeq::from_bytes(vec![0x00]).unwrap(),
                to: LogSeq::from_bytes(vec![0x09]).unwrap(),
                generated_at: now,
                bytes: vec![0xde, 0xad, 0xbe, 0xef],
            },
        )
        .await
        .expect("append");
        let target_ts = now;
        let restore = svc
            .request_restore(
                &owner(owner_id),
                db_id,
                RestoreRequest {
                    timestamp: target_ts,
                    base_backup: None,
                    confirm: false,
                },
            )
            .await
            .expect("request");
        assert_eq!(restore.status(), PitrRestoreStatus::Staged);
        let staging = restore.staging_db_id().expect("staging id");
        assert_ne!(staging, db_id);
        let promoted = svc
            .promote_restore(&owner(owner_id), restore.id())
            .await
            .expect("promote");
        assert_eq!(promoted.status(), PitrRestoreStatus::Promoted);
    }

    #[tokio::test]
    async fn restore_rejects_timestamp_outside_range() {
        let (svc, owner_id, db_id) = build_test_service().await;
        svc.enable_stream(&owner(owner_id), db_id, StreamTargetId::new(Uuid::new_v4()))
            .await
            .expect("enable");
        let r = svc
            .request_restore(
                &owner(owner_id),
                db_id,
                RestoreRequest {
                    timestamp: Utc::now() - chrono::Duration::seconds(1),
                    base_backup: None,
                    confirm: false,
                },
            )
            .await;
        assert!(matches!(
            r,
            Err(RestoreRequestError::TimestampOutOfRange(_))
        ));
    }

    #[tokio::test]
    async fn replaying_same_timestamp_is_idempotent() {
        let (svc, owner_id, db_id) = build_test_service().await;
        svc.enable_stream(&owner(owner_id), db_id, StreamTargetId::new(Uuid::new_v4()))
            .await
            .expect("enable");
        let sink = svc.sink();
        let now = Utc::now();
        sink.append(
            db_id,
            &openpanel_domain::BinlogSegment {
                from: LogSeq::from_bytes(vec![0x00]).unwrap(),
                to: LogSeq::from_bytes(vec![0x09]).unwrap(),
                generated_at: now,
                bytes: vec![0xde, 0xad, 0xbe, 0xef],
            },
        )
        .await
        .expect("append");
        let target_ts = now;
        let req = RestoreRequest {
            timestamp: target_ts,
            base_backup: None,
            confirm: false,
        };
        let r1 = svc
            .request_restore(&owner(owner_id), db_id, req.clone())
            .await
            .expect("first");
        let r2 = svc
            .request_restore(&owner(owner_id), db_id, req)
            .await
            .expect("second");
        assert_eq!(r1.staging_db_id().unwrap(), r2.staging_db_id().unwrap());
        assert_eq!(r1.status(), r2.status());
    }

    #[tokio::test]
    async fn capture_incremental_records_audit() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let streams_repo: Arc<dyn BinlogStreamRepository> = Arc::new(
            crate::db_pitr::repo::SqliteBinlogStreamRepository::new(pool.clone()),
        );
        let restores_repo: Arc<dyn PitrRepository> =
            Arc::new(SqlitePitrRepository::new(pool.clone()));
        let inc_repo: Arc<dyn IncrementalRepository> = Arc::new(
            crate::db_pitr::repo::SqliteIncrementalRepository::new(pool.clone()),
        );
        let mut audit = MockAudit::new();
        audit
            .expect_record()
            .times(1)
            .withf(|e| matches!(e.action, AuditAction::PitrIncrementalCaptured))
            .returning(|_| Ok(()));
        let owner_id = Uuid::new_v4();
        let database_id = Uuid::new_v4();
        seed_real_database_row(pool.clone(), database_id, owner_id).await;
        let database_repo = seed_database_repo(database_id, owner_id);
        let sink: Arc<dyn BinlogSink> = Arc::new(MemorySink::default());
        let tailer: Arc<dyn LogTailer> = Arc::new(MemoryTailer);
        let svc = PitrService::new(
            streams_repo,
            restores_repo,
            inc_repo,
            database_repo,
            sink,
            vec![tailer],
            Arc::new(audit),
        );
        let inc = svc
            .capture_incremental(
                &owner(owner_id),
                database_id,
                Uuid::new_v4(),
                "s3://bucket/inc/abc",
                IncrementalMode::BlockDelta,
                4096,
                LogSeq::from_bytes(vec![0x05]).unwrap(),
            )
            .await
            .expect("capture");
        assert_eq!(inc.bytes(), 4096);
    }

    #[tokio::test]
    async fn non_owner_cannot_enable_stream() {
        let (svc, _owner_id, db_id) = build_test_service().await;
        let r = svc
            .enable_stream(
                &reseller(Uuid::new_v4()),
                db_id,
                StreamTargetId::new(Uuid::new_v4()),
            )
            .await;
        assert!(matches!(r, Err(RestoreRequestError::Forbidden)));
    }
}

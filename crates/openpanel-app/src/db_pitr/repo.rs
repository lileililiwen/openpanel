//! SQLite-backed adapters for the `db-pitr` repository traits.
//!
//! The adapters are responsible for serialising `LogSeq` (a `Vec<u8>`
//! of engine bytes) to its hex representation and back, and for
//! round-tripping the typed enums (`BinlogStreamStatus`,
//! `PitrRestoreStatus`, `IncrementalMode`) to their wire strings.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    BinlogRange, BinlogStream, BinlogStreamRepository, BinlogStreamStatus, IncrementalBackup,
    IncrementalMode, IncrementalRepository, LogSeq, PitrError, PitrRepository, PitrRestore,
    PitrRestoreStatus, RestoreReplayWindow, RestoreTimestamp, StreamTargetId,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

fn parse_status(s: &str) -> Result<BinlogStreamStatus, PitrError> {
    match s {
        "active" => Ok(BinlogStreamStatus::Active),
        "paused" => Ok(BinlogStreamStatus::Paused),
        "broken" => Ok(BinlogStreamStatus::Broken),
        other => Err(PitrError::Invalid(format!(
            "unknown binlog stream status `{other}`"
        ))),
    }
}

fn parse_restore_status(s: &str) -> Result<PitrRestoreStatus, PitrError> {
    match s {
        "pending" => Ok(PitrRestoreStatus::Pending),
        "staged" => Ok(PitrRestoreStatus::Staged),
        "promoting" => Ok(PitrRestoreStatus::Promoting),
        "promoted" => Ok(PitrRestoreStatus::Promoted),
        "failed" => Ok(PitrRestoreStatus::Failed),
        other => Err(PitrError::Invalid(format!(
            "unknown PITR restore status `{other}`"
        ))),
    }
}

fn parse_mode(s: &str) -> Result<IncrementalMode, PitrError> {
    match s {
        "block_delta" => Ok(IncrementalMode::BlockDelta),
        "logical" => Ok(IncrementalMode::Logical),
        other => Err(PitrError::Invalid(format!(
            "unknown incremental mode `{other}`"
        ))),
    }
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, PitrError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| PitrError::Invalid(format!("invalid timestamp `{s}`: {e}")))
}

fn parse_optional_ts(s: Option<String>) -> Result<Option<DateTime<Utc>>, PitrError> {
    match s {
        Some(value) => parse_ts(&value).map(Some),
        None => Ok(None),
    }
}

fn parse_optional_uuid(s: Option<String>) -> Result<Option<Uuid>, PitrError> {
    match s {
        Some(value) => Uuid::parse_str(&value)
            .map(Some)
            .map_err(|e| PitrError::Invalid(format!("invalid uuid `{value}`: {e}"))),
        None => Ok(None),
    }
}

fn parse_optional_log_seq(s: Option<String>) -> Result<Option<LogSeq>, PitrError> {
    match s {
        Some(value) => LogSeq::from_hex(&value).map(Some),
        None => Ok(None),
    }
}

/// SQLite-backed implementation of [`BinlogStreamRepository`].
#[derive(Clone)]
pub struct SqliteBinlogStreamRepository {
    pool: Pool<Sqlite>,
}

impl SqliteBinlogStreamRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BinlogStreamRepository for SqliteBinlogStreamRepository {
    async fn insert(&self, stream: &BinlogStream) -> Result<(), PitrError> {
        sqlx::query(
            r#"INSERT INTO binlog_streams
                (id, database_id, target_id, last_flushed, continuous,
                 status, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(stream.id().to_string())
        .bind(stream.database_id().to_string())
        .bind(stream.target().as_uuid().to_string())
        .bind(stream.last_flushed().to_hex())
        .bind(if stream.continuous() { 1_i64 } else { 0_i64 })
        .bind(stream.status().as_str())
        .bind(stream.created_at().to_rfc3339())
        .bind(stream.updated_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<BinlogStream>, PitrError> {
        let row: Option<StreamRow> = sqlx::query_as::<_, StreamRow>(
            r#"SELECT id, database_id, target_id, last_flushed, continuous,
                      status, created_at, updated_at
                 FROM binlog_streams WHERE id = ?"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        row.map(StreamRow::into_stream).transpose()
    }

    async fn find_by_database(&self, database_id: Uuid) -> Result<Option<BinlogStream>, PitrError> {
        let row: Option<StreamRow> = sqlx::query_as::<_, StreamRow>(
            r#"SELECT id, database_id, target_id, last_flushed, continuous,
                      status, created_at, updated_at
                 FROM binlog_streams
                 WHERE database_id = ?
                 ORDER BY created_at DESC
                 LIMIT 1"#,
        )
        .bind(database_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        row.map(StreamRow::into_stream).transpose()
    }

    async fn list_all(&self) -> Result<Vec<BinlogStream>, PitrError> {
        let rows: Vec<StreamRow> = sqlx::query_as::<_, StreamRow>(
            r#"SELECT id, database_id, target_id, last_flushed, continuous,
                      status, created_at, updated_at
                 FROM binlog_streams
                 ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        rows.into_iter().map(StreamRow::into_stream).collect()
    }

    async fn update_state(
        &self,
        id: Uuid,
        last_flushed: &LogSeq,
        status: BinlogStreamStatus,
    ) -> Result<(), PitrError> {
        sqlx::query(
            r#"UPDATE binlog_streams
                  SET last_flushed = ?, status = ?, updated_at = ?
                  WHERE id = ?"#,
        )
        .bind(last_flushed.to_hex())
        .bind(status.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), PitrError> {
        sqlx::query("DELETE FROM binlog_streams WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| PitrError::Io(e.to_string()))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct StreamRow {
    id: String,
    database_id: String,
    target_id: String,
    last_flushed: String,
    continuous: i64,
    status: String,
    created_at: String,
    updated_at: String,
}

impl StreamRow {
    fn into_stream(self) -> Result<BinlogStream, PitrError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| PitrError::Invalid(format!("invalid id: {e}")))?;
        let database_id = Uuid::parse_str(&self.database_id)
            .map_err(|e| PitrError::Invalid(format!("invalid database_id: {e}")))?;
        let target_id = Uuid::parse_str(&self.target_id)
            .map_err(|e| PitrError::Invalid(format!("invalid target_id: {e}")))?;
        let last_flushed = LogSeq::from_hex(&self.last_flushed)?;
        let continuous = self.continuous != 0;
        let status = parse_status(&self.status)?;
        let created_at = parse_ts(&self.created_at)?;
        let updated_at = parse_ts(&self.updated_at)?;
        Ok(BinlogStream::restore(
            id,
            database_id,
            StreamTargetId::new(target_id),
            last_flushed,
            continuous,
            status,
            created_at,
            updated_at,
        ))
    }
}

/// SQLite-backed implementation of [`PitrRepository`].
#[derive(Clone)]
pub struct SqlitePitrRepository {
    pool: Pool<Sqlite>,
}

impl SqlitePitrRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PitrRepository for SqlitePitrRepository {
    async fn insert(&self, restore: &PitrRestore) -> Result<(), PitrError> {
        sqlx::query(
            r#"INSERT INTO pitr_restores
                (id, database_id, request_ts, base_backup, replay_to,
                 staging_db_id, status, requested_by, requested_at,
                 updated_at, failure_reason)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(restore.id().to_string())
        .bind(restore.database_id().to_string())
        .bind(restore.request_ts().to_string())
        .bind(restore.window().as_ref().map(|w| w.base_backup.to_string()))
        .bind(restore.window().as_ref().map(|w| w.replay_to.to_hex()))
        .bind(restore.staging_db_id().map(|id| id.to_string()))
        .bind(restore.status().as_str())
        .bind(restore.requested_by())
        .bind(restore.requested_at().to_rfc3339())
        .bind(restore.updated_at().to_rfc3339())
        .bind(restore.failure_reason().map(str::to_owned))
        .execute(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PitrRestore>, PitrError> {
        let row: Option<RestoreRow> = sqlx::query_as::<_, RestoreRow>(
            r#"SELECT id, database_id, request_ts, base_backup, replay_to,
                      staging_db_id, status, requested_by, requested_at,
                      updated_at, failure_reason
                 FROM pitr_restores WHERE id = ?"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        row.map(RestoreRow::into_restore).transpose()
    }

    async fn list_by_database(&self, database_id: Uuid) -> Result<Vec<PitrRestore>, PitrError> {
        let rows: Vec<RestoreRow> = sqlx::query_as::<_, RestoreRow>(
            r#"SELECT id, database_id, request_ts, base_backup, replay_to,
                      staging_db_id, status, requested_by, requested_at,
                      updated_at, failure_reason
                 FROM pitr_restores
                 WHERE database_id = ?
                 ORDER BY requested_at DESC"#,
        )
        .bind(database_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        rows.into_iter().map(RestoreRow::into_restore).collect()
    }

    async fn update(&self, restore: &PitrRestore) -> Result<(), PitrError> {
        sqlx::query(
            r#"UPDATE pitr_restores
                  SET request_ts = ?, base_backup = ?, replay_to = ?,
                      staging_db_id = ?, status = ?, requested_by = ?,
                      requested_at = ?, updated_at = ?, failure_reason = ?
                  WHERE id = ?"#,
        )
        .bind(restore.request_ts().to_string())
        .bind(restore.window().as_ref().map(|w| w.base_backup.to_string()))
        .bind(restore.window().as_ref().map(|w| w.replay_to.to_hex()))
        .bind(restore.staging_db_id().map(|id| id.to_string()))
        .bind(restore.status().as_str())
        .bind(restore.requested_by())
        .bind(restore.requested_at().to_rfc3339())
        .bind(restore.updated_at().to_rfc3339())
        .bind(restore.failure_reason().map(str::to_owned))
        .bind(restore.id().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct RestoreRow {
    id: String,
    database_id: String,
    request_ts: String,
    base_backup: Option<String>,
    replay_to: Option<String>,
    staging_db_id: Option<String>,
    status: String,
    requested_by: String,
    requested_at: String,
    updated_at: String,
    failure_reason: Option<String>,
}

impl RestoreRow {
    fn into_restore(self) -> Result<PitrRestore, PitrError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| PitrError::Invalid(format!("invalid id: {e}")))?;
        let database_id = Uuid::parse_str(&self.database_id)
            .map_err(|e| PitrError::Invalid(format!("invalid database_id: {e}")))?;
        let request_ts = parse_ts(&self.request_ts)?;
        let request_ts = RestoreTimestamp::from_stored(request_ts);
        let window = match (self.base_backup, self.replay_to) {
            (Some(b), Some(r)) => {
                let base_backup = Uuid::parse_str(&b)
                    .map_err(|e| PitrError::Invalid(format!("invalid base_backup: {e}")))?;
                let replay_to = LogSeq::from_hex(&r)?;
                Some(RestoreReplayWindow {
                    base_backup,
                    replay_to,
                })
            }
            _ => None,
        };
        let staging_db_id = parse_optional_uuid(self.staging_db_id)?;
        let status = parse_restore_status(&self.status)?;
        let requested_at = parse_ts(&self.requested_at)?;
        let updated_at = parse_ts(&self.updated_at)?;
        let _ = parse_optional_log_seq(None::<String>)?;
        Ok(PitrRestore::restore(
            id,
            database_id,
            request_ts,
            window,
            staging_db_id,
            status,
            self.requested_by,
            requested_at,
            updated_at,
            self.failure_reason,
        ))
    }
}

/// SQLite-backed implementation of [`IncrementalRepository`].
#[derive(Clone)]
pub struct SqliteIncrementalRepository {
    pool: Pool<Sqlite>,
}

impl SqliteIncrementalRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IncrementalRepository for SqliteIncrementalRepository {
    async fn insert(&self, inc: &IncrementalBackup) -> Result<(), PitrError> {
        sqlx::query(
            r#"INSERT INTO incremental_backups
                (id, database_id, base_backup, delta_ref, mode, bytes,
                 captured_through, captured_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(inc.id().to_string())
        .bind(inc.database_id().to_string())
        .bind(inc.base_backup().to_string())
        .bind(inc.delta_ref())
        .bind(inc.mode().as_str())
        .bind(inc.bytes() as i64)
        .bind(inc.captured_through().to_hex())
        .bind(inc.captured_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<IncrementalBackup>, PitrError> {
        let row: Option<IncrementalRow> = sqlx::query_as::<_, IncrementalRow>(
            r#"SELECT id, database_id, base_backup, delta_ref, mode, bytes,
                      captured_through, captured_at
                 FROM incremental_backups WHERE id = ?"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        row.map(IncrementalRow::into_incremental).transpose()
    }

    async fn list_by_database(
        &self,
        database_id: Uuid,
    ) -> Result<Vec<IncrementalBackup>, PitrError> {
        let rows: Vec<IncrementalRow> = sqlx::query_as::<_, IncrementalRow>(
            r#"SELECT id, database_id, base_backup, delta_ref, mode, bytes,
                      captured_through, captured_at
                 FROM incremental_backups
                 WHERE database_id = ?
                 ORDER BY captured_at DESC"#,
        )
        .bind(database_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PitrError::Io(e.to_string()))?;
        rows.into_iter()
            .map(IncrementalRow::into_incremental)
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct IncrementalRow {
    id: String,
    database_id: String,
    base_backup: String,
    delta_ref: String,
    mode: String,
    bytes: i64,
    captured_through: String,
    captured_at: String,
}

impl IncrementalRow {
    fn into_incremental(self) -> Result<IncrementalBackup, PitrError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| PitrError::Invalid(format!("invalid id: {e}")))?;
        let database_id = Uuid::parse_str(&self.database_id)
            .map_err(|e| PitrError::Invalid(format!("invalid database_id: {e}")))?;
        let base_backup = Uuid::parse_str(&self.base_backup)
            .map_err(|e| PitrError::Invalid(format!("invalid base_backup: {e}")))?;
        let mode = parse_mode(&self.mode)?;
        let bytes = u64::try_from(self.bytes)
            .map_err(|e| PitrError::Invalid(format!("invalid bytes: {e}")))?;
        let captured_through = LogSeq::from_hex(&self.captured_through)?;
        let captured_at = parse_ts(&self.captured_at)?;
        Ok(IncrementalBackup::restore(
            id,
            database_id,
            base_backup,
            self.delta_ref,
            mode,
            bytes,
            captured_through,
            captured_at,
        ))
    }
}

/// Convenience: an empty [`BinlogRange`] used when no segments have
/// been streamed for a database yet.
pub fn empty_range(now: DateTime<Utc>) -> BinlogRange {
    BinlogRange::new(LogSeq::initial(), LogSeq::initial(), now, now).unwrap_or_else(|_| {
        // BinlogRange::new only fails on end < start, and we pass now==now.
        unreachable!("empty range with equal timestamps is always valid")
    })
}

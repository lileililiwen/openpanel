//! `BinlogSink` (storage-side) and `LogTailer` (engine-side)
//! abstractions used by the application service.
//!
//! The application service drives the streaming flow:
//!
//! 1. `LogTailer::tail(from)` returns the next binlog segment after
//!    the supplied position.
//! 2. The service writes that segment to a `BinlogSink` and
//!    checkpoints the new `LogSeq`.
//! 3. On restore, the service reads the segments back from the
//!    `BinlogSink`, asks the `LogTailer` to replay them up to the
//!    target timestamp, and lands the result in a staging database.
//!
//! Production code wires the `MySqlBinlogTailer` and an S3 / local
//! `BinlogSink`; tests wire in-memory implementations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::db_pitr::{BinlogRange, LogSeq, PitrError};

/// A single transaction-log segment fetched from the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinlogSegment {
    /// Position this segment starts at.
    pub from: LogSeq,
    /// Position this segment ends at.
    pub to: LogSeq,
    /// Wall-clock instant the engine generated this segment at.
    pub generated_at: DateTime<Utc>,
    /// Raw segment bytes. The domain treats them as opaque; the
    /// engine adapter is responsible for ordering and decoding.
    pub bytes: Vec<u8>,
}

/// Storage-side abstraction: the destination of streamed transaction
/// log segments. Implementations are responsible for durability,
/// retention, and (in production) encryption-at-rest.
#[async_trait]
pub trait BinlogSink: Send + Sync + 'static {
    /// Append a segment. Returns the durable position the segment
    /// was committed at (typically `to`). The service checkpoints
    /// this as `last_flushed`.
    async fn append(
        &self,
        database_id: uuid::Uuid,
        segment: &BinlogSegment,
    ) -> Result<LogSeq, PitrError>;

    /// Read the segments needed to replay from `earliest` up to
    /// `target_ts`. The returned list is ordered by `from`.
    async fn replay_window(
        &self,
        database_id: uuid::Uuid,
        earliest: LogSeq,
        target_ts: DateTime<Utc>,
    ) -> Result<Vec<BinlogSegment>, PitrError>;

    /// Inspect the available range for `database_id`. Returns
    /// `BinlogRange::new(initial, initial, now, now)` (with
    /// `empty = true`) if no segments have ever been streamed.
    async fn available_range(&self, database_id: uuid::Uuid) -> Result<BinlogRange, PitrError>;
}

/// Engine-side abstraction: tails transaction-log segments from a
/// running database. Production code uses the `mysqlbinlog` CLI for
/// MySQL/MariaDB and the `archive_command` for PostgreSQL; tests
/// use a hand-written fake.
#[async_trait]
pub trait LogTailer: Send + Sync + 'static {
    /// Fetch the segment that starts at or after `from`. Returns
    /// `None` if the engine is fully caught up.
    async fn next_segment(
        &self,
        database_id: uuid::Uuid,
        from: LogSeq,
    ) -> Result<Option<BinlogSegment>, PitrError>;

    /// Mark the database as having PITR streaming enabled. This
    /// causes the engine to start writing binlog rows / WAL
    /// segments. Idempotent.
    async fn enable_streaming(&self, database_id: uuid::Uuid) -> Result<(), PitrError>;

    /// Mark the database as having PITR streaming disabled. The
    /// engine stops producing new segments.
    async fn disable_streaming(&self, database_id: uuid::Uuid) -> Result<(), PitrError>;
}

/// Outcome of a replay-window computation. The service uses this to
/// decide whether a restore can proceed (`Ready`), the caller must
/// wait (`Partial`), or the timestamp is outside the available
/// window (`OutOfRange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// The window is fully available.
    Ready,
    /// Some segments are present but the target is beyond the
    /// latest — the engine is still streaming. The caller may
    /// retry later.
    Partial,
    /// The target is outside the available window entirely.
    OutOfRange,
}

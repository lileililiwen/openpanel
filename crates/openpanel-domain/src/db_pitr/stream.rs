//! `BinlogStream` aggregate: tracks a database's continuous
//! transaction-log streaming lifecycle.
//!
//! A stream is `Active` while the engine tail produces segments and
//! the sink durably persists them. It transitions to `Paused` when
//! the operator disables it, to `Broken` when the engine or sink
//! reports an unrecoverable error, and back to `Active` when the
//! operator (or the streamer task) re-enables it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db_pitr::{LogSeq, PitrError};

/// Opaque identifier for a destination target. The eventual
/// `offsite-backup-targets` change will replace this with a typed
/// `BackupTargetId`; for now the value is a `Uuid` and the future
/// change performs a mechanical rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamTargetId(pub Uuid);

impl StreamTargetId {
    /// Build from a UUID.
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    /// The underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for StreamTargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Lifecycle of a `BinlogStream`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinlogStreamStatus {
    /// The streamer is running.
    Active,
    /// The operator disabled the streamer.
    Paused,
    /// The streamer hit an error and stopped.
    Broken,
}

impl BinlogStreamStatus {
    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            BinlogStreamStatus::Active => "active",
            BinlogStreamStatus::Paused => "paused",
            BinlogStreamStatus::Broken => "broken",
        }
    }
}

/// A continuous binlog / WAL stream for a single database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinlogStream {
    id: Uuid,
    database_id: Uuid,
    target: StreamTargetId,
    last_flushed: LogSeq,
    continuous: bool,
    status: BinlogStreamStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl BinlogStream {
    /// Create a new stream. The initial checkpoint is `LogSeq::initial()`
    /// and the status is `Active`.
    pub fn new(
        id: Uuid,
        database_id: Uuid,
        target: StreamTargetId,
        continuous: bool,
        now: DateTime<Utc>,
    ) -> Result<Self, PitrError> {
        if database_id.is_nil() {
            return Err(PitrError::Invalid("database id is nil".into()));
        }
        Ok(Self {
            id,
            database_id,
            target,
            last_flushed: LogSeq::initial(),
            continuous,
            status: BinlogStreamStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }

    /// Reconstruct from storage, bypassing validation.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        database_id: Uuid,
        target: StreamTargetId,
        last_flushed: LogSeq,
        continuous: bool,
        status: BinlogStreamStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            database_id,
            target,
            last_flushed,
            continuous,
            status,
            created_at,
            updated_at,
        }
    }

    /// Mark the stream paused (operator action).
    pub fn pause(&mut self, now: DateTime<Utc>) -> Result<(), PitrError> {
        if self.status == BinlogStreamStatus::Paused {
            return Ok(());
        }
        self.status = BinlogStreamStatus::Paused;
        self.updated_at = now;
        Ok(())
    }

    /// Resume a paused or broken stream.
    pub fn resume(&mut self, now: DateTime<Utc>) -> Result<(), PitrError> {
        if self.status == BinlogStreamStatus::Active {
            return Ok(());
        }
        self.status = BinlogStreamStatus::Active;
        self.updated_at = now;
        Ok(())
    }

    /// Mark the stream broken (engine or sink I/O failure).
    pub fn mark_broken(&mut self, now: DateTime<Utc>) -> Result<(), PitrError> {
        if self.status == BinlogStreamStatus::Broken {
            return Ok(());
        }
        self.status = BinlogStreamStatus::Broken;
        self.updated_at = now;
        Ok(())
    }

    /// Checkpoint the last successfully flushed position. The
    /// position MUST be equal to or after the current checkpoint.
    pub fn checkpoint(&mut self, seq: LogSeq, now: DateTime<Utc>) {
        self.last_flushed = seq;
        self.updated_at = now;
    }

    /// Stream id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Database id this stream is attached to.
    pub fn database_id(&self) -> Uuid {
        self.database_id
    }

    /// Destination target.
    pub fn target(&self) -> StreamTargetId {
        self.target
    }

    /// Last flushed position.
    pub fn last_flushed(&self) -> &LogSeq {
        &self.last_flushed
    }

    /// Whether this stream is continuous (vs. periodic flush).
    pub fn continuous(&self) -> bool {
        self.continuous
    }

    /// Current status.
    pub fn status(&self) -> BinlogStreamStatus {
        self.status
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Last update timestamp.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn new_starts_active() {
        let now = ts();
        let s = BinlogStream::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            StreamTargetId::new(Uuid::new_v4()),
            true,
            now,
        )
        .unwrap();
        assert_eq!(s.status(), BinlogStreamStatus::Active);
        assert!(s.continuous());
        assert_eq!(s.last_flushed(), &LogSeq::initial());
    }

    #[test]
    fn pause_resume_broken() {
        let now = ts();
        let mut s = BinlogStream::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            StreamTargetId::new(Uuid::new_v4()),
            true,
            now,
        )
        .unwrap();
        s.pause(now).unwrap();
        assert_eq!(s.status(), BinlogStreamStatus::Paused);
        s.resume(now).unwrap();
        assert_eq!(s.status(), BinlogStreamStatus::Active);
        s.mark_broken(now).unwrap();
        assert_eq!(s.status(), BinlogStreamStatus::Broken);
        s.resume(now).unwrap();
        assert_eq!(s.status(), BinlogStreamStatus::Active);
    }

    #[test]
    fn rejects_nil_database() {
        let r = BinlogStream::new(
            Uuid::new_v4(),
            Uuid::nil(),
            StreamTargetId::new(Uuid::new_v4()),
            true,
            ts(),
        );
        assert!(matches!(r, Err(PitrError::Invalid(_))));
    }
}

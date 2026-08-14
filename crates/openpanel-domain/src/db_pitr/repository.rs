//! Repository traits for the `db-pitr` bounded context. Implementations
//! (SQLite, future remote variants) live in `openpanel-app`.

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    databases::{database::Database, error::DatabaseError},
    db_pitr::{BinlogStream, IncrementalBackup, PitrError, PitrRestore, StreamTargetId},
};

/// Minimal database lookup contract used by the PITR service.
/// `DatabaseRepository` already covers this; the slim trait lets
/// composition code pass either the full repository or a focused
/// adapter.
#[async_trait]
pub trait DatabaseLookup: Send + Sync + 'static {
    /// Look up a database by id.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Database>, DatabaseError>;
}

/// Persistence contract for `BinlogStream` aggregates.
#[async_trait]
pub trait BinlogStreamRepository: Send + Sync + 'static {
    /// Insert a new stream.
    async fn insert(&self, stream: &BinlogStream) -> Result<(), PitrError>;

    /// Look up a stream by its unique identifier.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<BinlogStream>, PitrError>;

    /// Look up the (at most one) active or paused stream for a
    /// database. Returns `Ok(None)` if no stream is configured.
    async fn find_by_database(&self, database_id: Uuid) -> Result<Option<BinlogStream>, PitrError>;

    /// List every stream currently configured (any status).
    async fn list_all(&self) -> Result<Vec<BinlogStream>, PitrError>;

    /// Persist a checkpointed `last_flushed` and status update.
    async fn update_state(
        &self,
        id: Uuid,
        last_flushed: &crate::db_pitr::LogSeq,
        status: crate::db_pitr::BinlogStreamStatus,
    ) -> Result<(), PitrError>;

    /// Remove a stream.
    async fn delete(&self, id: Uuid) -> Result<(), PitrError>;
}

/// Persistence contract for `PitrRestore` aggregates.
#[async_trait]
pub trait PitrRepository: Send + Sync + 'static {
    /// Insert a new restore.
    async fn insert(&self, restore: &PitrRestore) -> Result<(), PitrError>;

    /// Look up by id.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<PitrRestore>, PitrError>;

    /// List every restore for a database, newest first.
    async fn list_by_database(&self, database_id: Uuid) -> Result<Vec<PitrRestore>, PitrError>;

    /// Persist status / window / staging / failure-reason changes.
    async fn update(&self, restore: &PitrRestore) -> Result<(), PitrError>;
}

/// Persistence contract for `IncrementalBackup` aggregates.
#[async_trait]
pub trait IncrementalRepository: Send + Sync + 'static {
    /// Insert a new incremental capture.
    async fn insert(&self, inc: &IncrementalBackup) -> Result<(), PitrError>;

    /// Look up by id.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<IncrementalBackup>, PitrError>;

    /// List every incremental capture for a database, newest first.
    async fn list_by_database(
        &self,
        database_id: Uuid,
    ) -> Result<Vec<IncrementalBackup>, PitrError>;
}

/// Marker wrapper so callers can name the *target id* type used by
/// streams without re-importing from the stream module. The eventual
/// `offsite-backup-targets` change will replace this with a typed
/// alias to `BackupTargetId`; for now the value is a `Uuid`.
pub type TargetId = StreamTargetId;

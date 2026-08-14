//! Incremental file-backup helper.
//!
//! [`IncrementalFileLayer`] is the thin shim the application service
//! calls when it wants to capture a block-level delta of a database
//! data directory. The actual block-level capture is the engine
//! adapter's job (out of scope for this change); this layer just
//! records the metadata in the [`IncrementalRepository`] so the
//! schedule and audit trail line up.

use std::sync::Arc;

use openpanel_domain::{
    IncrementalBackup, IncrementalMode, IncrementalRepository, LogSeq, PitrError,
};
use uuid::Uuid;

/// Persists incremental capture metadata.
pub struct IncrementalFileLayer {
    incrementals: Arc<dyn IncrementalRepository>,
}

impl IncrementalFileLayer {
    /// Build a new layer over the given repository.
    pub fn new(incrementals: Arc<dyn IncrementalRepository>) -> Self {
        Self { incrementals }
    }

    /// Record a new incremental capture. Returns the persisted
    /// aggregate. The block-level capture itself is the engine
    /// adapter's job; this layer only persists the metadata.
    pub async fn record(
        &self,
        id: Uuid,
        database_id: Uuid,
        base_backup: Uuid,
        delta_ref: impl Into<String>,
        mode: IncrementalMode,
        bytes: u64,
        captured_through: LogSeq,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<IncrementalBackup, PitrError> {
        let inc = IncrementalBackup::new(
            id,
            database_id,
            base_backup,
            delta_ref,
            mode,
            bytes,
            captured_through,
            now,
        )?;
        self.incrementals.insert(&inc).await?;
        Ok(inc)
    }
}

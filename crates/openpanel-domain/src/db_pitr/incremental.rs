//! Incremental file-backup mode: a block-level delta of the database
//! data directory captured alongside a full dump.
//!
//! Each `IncrementalBackup` references the full backup it is
//! relative to (`base_backup`) and the artifact holding the
//! block-level delta (`delta_ref`). The application service is
//! responsible for materialising the delta via the engine adapter
//! and uploading it to the destination target.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db_pitr::{LogSeq, PitrError};

/// The kind of incremental capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncrementalMode {
    /// A block-level delta of the data directory.
    BlockDelta,
    /// A logical-delta (statement / row) capture. Currently unused;
    /// reserved for engines that do not expose block-level access.
    Logical,
}

impl IncrementalMode {
    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            IncrementalMode::BlockDelta => "block_delta",
            IncrementalMode::Logical => "logical",
        }
    }
}

/// An incremental file-backup capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncrementalBackup {
    id: Uuid,
    database_id: Uuid,
    base_backup: Uuid,
    delta_ref: String,
    mode: IncrementalMode,
    bytes: u64,
    captured_through: LogSeq,
    captured_at: DateTime<Utc>,
}

impl IncrementalBackup {
    /// Record a new incremental capture.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        database_id: Uuid,
        base_backup: Uuid,
        delta_ref: impl Into<String>,
        mode: IncrementalMode,
        bytes: u64,
        captured_through: LogSeq,
        now: DateTime<Utc>,
    ) -> Result<Self, PitrError> {
        let delta_ref = delta_ref.into();
        if database_id.is_nil() {
            return Err(PitrError::Invalid("database id is nil".into()));
        }
        if base_backup.is_nil() {
            return Err(PitrError::Invalid("base backup id is nil".into()));
        }
        if delta_ref.trim().is_empty() {
            return Err(PitrError::Invalid("delta ref is empty".into()));
        }
        Ok(Self {
            id,
            database_id,
            base_backup,
            delta_ref,
            mode,
            bytes,
            captured_through,
            captured_at: now,
        })
    }

    /// Reconstruct from storage, bypassing validation.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        database_id: Uuid,
        base_backup: Uuid,
        delta_ref: String,
        mode: IncrementalMode,
        bytes: u64,
        captured_through: LogSeq,
        captured_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            database_id,
            base_backup,
            delta_ref,
            mode,
            bytes,
            captured_through,
            captured_at,
        }
    }

    /// Incremental id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Database id.
    pub fn database_id(&self) -> Uuid {
        self.database_id
    }

    /// Full backup this delta is relative to.
    pub fn base_backup(&self) -> Uuid {
        self.base_backup
    }

    /// Storage reference (e.g. S3 key) for the delta artifact.
    pub fn delta_ref(&self) -> &str {
        &self.delta_ref
    }

    /// Mode used for this capture.
    pub fn mode(&self) -> IncrementalMode {
        self.mode
    }

    /// Number of bytes captured.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The `LogSeq` the delta was captured through.
    pub fn captured_through(&self) -> &LogSeq {
        &self.captured_through
    }

    /// When the delta was captured.
    pub fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let now = Utc::now();
        let inc = IncrementalBackup::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "s3://bucket/inc/abc",
            IncrementalMode::BlockDelta,
            1024,
            LogSeq::from_bytes(vec![0x07]).unwrap(),
            now,
        )
        .unwrap();
        assert_eq!(inc.delta_ref(), "s3://bucket/inc/abc");
        assert_eq!(inc.mode(), IncrementalMode::BlockDelta);
        assert_eq!(inc.bytes(), 1024);
    }

    #[test]
    fn rejects_empty_delta_ref() {
        let now = Utc::now();
        let r = IncrementalBackup::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "  ",
            IncrementalMode::BlockDelta,
            0,
            LogSeq::initial(),
            now,
        );
        assert!(matches!(r, Err(PitrError::Invalid(_))));
    }
}

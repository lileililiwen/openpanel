//! Database point-in-time recovery bounded context.
//!
//! Continuous transaction-log streaming, point-in-time restore to an
//! arbitrary timestamp, and incremental file-backup deltas. All I/O
//! lives in `openpanel-app`; this crate contributes the aggregates,
//! value objects, repository **traits**, and the engine-side and
//! sink-side abstractions used by the application service.

/// `PitrError` error type.
pub mod error;
/// Incremental file-backup delta aggregate.
pub mod incremental;
/// Transaction-log sequence number, replay-window value object, and
/// stream / restore aggregates.
pub mod log_seq;
/// Repository traits for stream, restore, and incremental aggregates.
pub mod repository;
/// Point-in-time restore aggregate and status.
pub mod restore;
/// `BinlogSink` and `LogTailer` engine / storage abstractions.
pub mod sink;
/// BinlogStream aggregate and its lifecycle.
pub mod stream;

pub use error::PitrError;
pub use incremental::{IncrementalBackup, IncrementalMode};
pub use log_seq::{BinlogRange, LogSeq, RestoreTimestamp};
pub use repository::{
    BinlogStreamRepository, DatabaseLookup, IncrementalRepository, PitrRepository,
};
pub use restore::{PitrRestore, PitrRestoreStatus, RestoreReplayWindow};
pub use sink::{BinlogSegment, BinlogSink, LogTailer, ReplayOutcome};
pub use stream::{BinlogStream, BinlogStreamStatus, StreamTargetId};

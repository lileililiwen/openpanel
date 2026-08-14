//! Database point-in-time recovery bounded context: binlog streaming,
//! point-in-time restore, and incremental file-backup deltas.
//!
//! The application service [`PitrService`] drives the streaming and
//! restore flows. Storage adapters implement [`BinlogSink`] and
//! engine adapters implement [`LogTailer`]; production code wires
//! the S3-compatible sink and `mysqlbinlog` tailer, while tests wire
//! in-memory fakes.

pub mod in_memory;
pub mod incremental;
pub mod module;
pub mod repo;
pub mod service;
pub mod streamer;

pub use in_memory::{InMemoryBinlogSink, InMemoryLogTailer};
pub use incremental::IncrementalFileLayer;
pub use module::{DbPitrModule, MODULE_NAME};
pub use service::{PitrService, RestoreRequest, RestoreRequestError};
pub use streamer::BinlogStreamer;

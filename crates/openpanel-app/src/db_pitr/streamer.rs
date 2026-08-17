//! Background task scaffolding for continuous binlog streaming.
//!
//! Production code spawns a `BinlogStreamer` per active stream;
//! the streamer polls the engine `LogTailer`, writes segments to
//! the `BinlogSink`, and checkpoints the resulting `LogSeq` in
//! the database. The full background task lives behind a feature
//! flag in the composition root; this module exposes the type so
//! composition code can wire it without `openpanel-app` taking a
//! dependency on `tokio` task spawning at the service layer.

use std::{sync::Arc, time::Duration};

use openpanel_core::AuditService;
use openpanel_domain::{BinlogSink, LogTailer};
use tokio::sync::Notify;
use uuid::Uuid;

use crate::db_pitr::service::PitrService;

/// Background streamer for a single database.
///
/// The type is intentionally `Send + Sync` and stores only
/// references; the actual loop body lives in the binary entry
/// points so the library code stays IO-agnostic.
pub struct BinlogStreamer {
    database_id: Uuid,
    // Consumed by the binary entry point loop; library code stays IO-agnostic.
    #[allow(dead_code)]
    service: Arc<PitrService>,
    #[allow(dead_code)]
    tailer: Arc<dyn LogTailer>,
    #[allow(dead_code)]
    sink: Arc<dyn BinlogSink>,
    #[allow(dead_code)]
    audit: Arc<dyn AuditService>,
    tick: Duration,
    shutdown: Arc<Notify>,
}

impl BinlogStreamer {
    /// Build a new streamer with the given dependencies.
    pub fn new(
        database_id: Uuid,
        service: Arc<PitrService>,
        tailer: Arc<dyn LogTailer>,
        sink: Arc<dyn BinlogSink>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            database_id,
            service,
            tailer,
            sink,
            audit,
            tick: Duration::from_secs(2),
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Set the interval between polls. Defaults to 2 seconds.
    pub fn with_tick(mut self, tick: Duration) -> Self {
        self.tick = tick;
        self
    }

    /// Get a handle that can stop the streamer.
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        self.shutdown.clone()
    }

    /// Notify the streamer to stop. The actual loop is owned by the
    /// binary entry point.
    pub fn stop(&self) {
        self.shutdown.notify_waiters();
    }

    /// The database this streamer is bound to.
    pub fn database_id(&self) -> Uuid {
        self.database_id
    }

    /// The polling tick.
    pub fn tick(&self) -> Duration {
        self.tick
    }
}

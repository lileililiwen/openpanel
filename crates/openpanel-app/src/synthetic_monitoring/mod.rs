//! Synthetic monitoring application layer: SQLite repository,
//! probe runner, scheduler, and SSL inspector.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, SyntheticMonitoringModule};
pub use repo::SqliteSyntheticRepository;
pub use service::{
    CheckOutcome, CheckRunner, HttpProbe, ProbeScheduler, RecordingProbe, SslExpiryInspector,
    TcpProbe,
};

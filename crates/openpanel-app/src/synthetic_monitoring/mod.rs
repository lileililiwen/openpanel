//! Synthetic monitoring application layer: SQLite repository,
//! probe runner, scheduler, SSL inspector, and public status page.

mod module;
mod repo;
mod service;
mod status_page_repo;
mod status_page_service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, SyntheticMonitoringModule};
pub use repo::SqliteSyntheticRepository;
pub use service::{
    CheckOutcome, CheckRunner, HttpProbe, NativeTlsSslInspector, ProbeScheduler, RecordingProbe,
    ReqwestHttpProbe, SslExpiryInspector, TcpProbe, TokioTcpProbe,
};
pub use status_page_repo::SqliteStatusPageRepository;
pub use status_page_service::{EntryView, PublicStatusView, StatusPageService, random_slug};

//! Web application installer bounded context: the typed
//! `InstallPlan` / `InstallRun` orchestration, SQLite repository,
//! and filesystem / artifact ports.

mod repo;
mod service;

pub use repo::SqliteWebApplicationInstallerRepository;
pub use service::{
    ArtifactDownloader, InstallerFs, RealInstallerFs, ReqwestArtifactDownloader,
    WebApplicationInstallerService,
};

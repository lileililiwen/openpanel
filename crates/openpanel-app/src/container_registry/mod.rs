//! Container registry bounded context: SQLite-backed repositories,
//! storage layout, retention, scan hook, and registry service.

pub mod module;
pub mod repo;
pub mod scan;
pub mod service;
pub mod storage;
#[cfg(test)]
mod service_tests;

pub use module::{ContainerRegistryModule, MODULE_NAME};
pub use repo::{SqliteImageRepository, SqliteNamespaceRepository, SqliteScanResultRepository};
pub use scan::{NoopScanHook, ScanHook, ScanHookError};
pub use service::{
    ContainerRegistryService, ImageBlob, PushRequest, PushResult, PushError,
};
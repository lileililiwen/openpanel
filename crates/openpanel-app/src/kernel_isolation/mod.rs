//! Kernel isolation application layer: SQLite repository,
//! cgroup enforcer, namespace isolator, quota bridge, and
//! module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{KernelIsolationModule, MODULE_NAME};
pub use repo::SqliteIsolationRepository;
pub use service::{
    CgroupEnforcer, CgroupWriter, NamespaceIsolator, QuotaBridge, RecordingCgroupWriter,
};
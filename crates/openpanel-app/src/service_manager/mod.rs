//! Service manager application layer: SQLite repository, lister,
//! actor, and module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, ServiceManagerModule};
pub use repo::SqliteServiceManagerRepository;
pub use service::{
    CommandOutput, RealSystemCtl, RecordingSystemCtl, ServiceActor, ServiceLister, SystemCtl,
};

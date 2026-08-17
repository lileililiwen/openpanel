//! Scheduled maintenance windows application layer: SQLite
//! repository, enforcer, and module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, MaintenanceWindowsModule};
pub use repo::SqliteMaintenanceRepository;
pub use service::MaintenanceEnforcer;

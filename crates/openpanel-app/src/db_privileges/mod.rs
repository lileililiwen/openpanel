//! Database privilege application layer: SQLite repository,
//! privilege service, remote access controller, admin-tool SSO,
//! and module composition.

pub mod grant_port;
mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use grant_port::{MemoryGrantPort, MySqlShellGrantPort};
pub use module::{DbPrivilegeModule, MODULE_NAME};
pub use repo::SqliteDbPrivilegeRepository;
pub use service::{
    AdminToolSso, DbRemoteAccessContext, MySqlGrantPort, PrivilegeService, RemoteAccessController,
};

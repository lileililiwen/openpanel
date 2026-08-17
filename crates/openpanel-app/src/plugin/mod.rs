//! Plugin extension framework bounded context: the SQLite-backed
//! `PluginRegistry` adapter and the install / lifecycle service.
//!
//! The framework layer is the *base* that the marketplace layer
//! (`openpanel-app::plugin_marketplace`) sits on top of. The
//! marketplace calls `PluginService::install_manifest` after it
//! has verified a manifest against the marketplace CA; the base
//! service records the install in the `installed_plugins` table
//! and writes an audit event.

pub mod module;
pub mod repo;
pub mod service;
#[cfg(test)]
mod service_tests;

pub use module::{MODULE_NAME, PluginModule};
pub use repo::SqlitePluginRegistry;
pub use service::{PluginInstallError, PluginService};

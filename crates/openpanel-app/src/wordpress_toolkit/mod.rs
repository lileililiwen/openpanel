//! WordPress toolkit application layer: SQLite repository,
//! scanner, updater, cache layer, and module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, WordPressToolkitModule};
pub use repo::SqliteWpRepository;
pub use service::{FakeWpFilesystem, WpCacheLayer, WpScanner, WpToolkitService, WpUpdater};

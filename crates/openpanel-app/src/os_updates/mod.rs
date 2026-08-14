//! OS updates application layer: SQLite repository, lister, applier,
//! unattended config, and module composition.

mod module;
mod parser;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, OsUpdateModule};
pub use parser::parse_apt_dry_run;
pub use repo::SqliteOsUpdateRepository;
pub use service::{
    AptPackageManager, CommandOutput, OsUpdateApplier, OsUpdateLister, PackageManager,
    RecordingPackageManager, UnattendedConfig,
};

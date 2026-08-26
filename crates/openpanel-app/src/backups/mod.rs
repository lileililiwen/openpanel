//! Backup/restore orchestration, local destination, and module wiring.

pub mod module;
pub mod sandbox;
pub mod server_snapshot;
pub mod service;
pub mod snapshot_importer;

pub use module::BackupsModule;
pub use service::{
    BackupPlanInput, BackupPlanUpdate, BackupService, BackupServiceError, RestoreInput,
    RestorePreview,
};
pub use snapshot_importer::{
    SkipAll, SnapshotBundleSource, SnapshotImporterDriver, SnapshotTranslator, export_to_target,
    import_from_target,
};

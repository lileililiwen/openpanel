//! Backup/restore orchestration, local destination, and module wiring.

pub mod backup_health;
pub mod drill_service;
pub mod module;
pub mod remote_verify;
pub mod sandbox;
pub mod server_snapshot;
pub mod service;
pub mod snapshot_importer;

pub use backup_health::{latest_completed_run, plan_health_from_runs, plan_health_id};
pub use drill_service::{DrillService, DrillServiceError};
pub use module::BackupsModule;
pub use remote_verify::{RemoteRoundtrip, roundtrip_probe_key, verify_remote_roundtrip};
pub use service::{
    BackupPlanInput, BackupPlanUpdate, BackupService, BackupServiceError, RestoreInput,
    RestorePreview,
};
pub use snapshot_importer::{
    SkipAll, SnapshotBundleSource, SnapshotImporterDriver, SnapshotTranslator, export_to_target,
    import_from_target,
};

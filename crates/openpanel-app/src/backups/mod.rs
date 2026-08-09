//! Backup/restore orchestration, local destination, and module wiring.

pub mod module;
pub mod service;

pub use module::BackupsModule;
pub use service::{
    BackupPlanInput, BackupPlanUpdate, BackupService, BackupServiceError, RestoreInput,
    RestorePreview,
};

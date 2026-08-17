//! Offsite backup targets bounded context: KEK management,
//! encrypted credential persistence, and the upload service that
//! drives a `BackupTargetAdapter` on behalf of a plan.

pub(crate) mod kek;
mod module;
mod repo;
mod service;

pub use kek::{
    decrypt_payload, derive_kek, encrypt_payload, master_key_fingerprint, unwrap_kek, wrap_kek,
};
pub use module::{MODULE_NAME, OffsiteBackupTargetsModule};
pub use repo::SqliteOffsiteBackupRepository;
pub use service::BackupUploadService;

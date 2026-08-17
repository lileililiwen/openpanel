//! Offsite backup targets module: composition root for the bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::offsite_backup_targets::{BackupUploadService, SqliteOffsiteBackupRepository};

/// Stable module name.
pub const MODULE_NAME: &str = "offsite_backup_targets";

/// Offsite backup targets module wires the SQLite adapter, the
/// upload service, and the migration.
pub struct OffsiteBackupTargetsModule {
    service: Arc<BackupUploadService>,
    migrations: Vec<Migration>,
}

impl OffsiteBackupTargetsModule {
    /// Build the module from the panel application context. The
    /// backup KEK is supplied by the caller (derived once at panel
    /// init from the operator passphrase).
    pub async fn with_kek(ctx: &AppContext, kek: [u8; 32]) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteOffsiteBackupRepository::new(pool));
        let service = Arc::new(BackupUploadService::with_kek(repo, ctx.audit.clone(), kek));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "encrypted backup credentials, remote targets, KEK wrappers"
                    .to_string(),
                sql: crate::migrations::OFFSITE_BACKUP_TARGETS_V001.to_string(),
            }],
        }
    }

    /// Build without a KEK (service still usable for list/delete of
    /// credentials that do not require decryption).
    pub async fn new(ctx: &AppContext) -> Self {
        Self::with_kek(ctx, [0u8; 32]).await
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<BackupUploadService> {
        self.service.clone()
    }
}

impl Module for OffsiteBackupTargetsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
}

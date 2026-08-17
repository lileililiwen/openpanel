//! Migration importers module: composition root for the bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::migration_importers::{MigrationService, SqliteMigrationRepository};

/// Stable module name.
pub const MODULE_NAME: &str = "migration_importers";

/// Migration importers module wires the SQLite adapter, the
/// application service, and the migration.
pub struct MigrationImportersModule {
    service: Arc<MigrationService>,
    migrations: Vec<Migration>,
}

impl MigrationImportersModule {
    /// Build the module from the panel application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteMigrationRepository::new(pool));
        let service = Arc::new(MigrationService::new(repo, ctx.audit.clone()));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "migration runs, imported resources, translation log".to_string(),
                sql: crate::migrations::MIGRATION_IMPORTERS_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<MigrationService> {
        self.service.clone()
    }
}

impl Module for MigrationImportersModule {
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

//! Quotas module: composition root for the bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::UserRepository;

use crate::identity::repo::SqliteUserRepository;
use crate::quotas::{QuotaService, SqliteQuotaRepository};

/// Stable module name.
pub const MODULE_NAME: &str = "quotas";

/// Quotas module wires the SQLite adapter, the application service,
/// and the migration.
pub struct QuotasModule {
    service: Arc<QuotaService>,
    migrations: Vec<Migration>,
}

impl QuotasModule {
    /// Build the module from the panel application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteQuotaRepository::new(pool.clone()));
        let users: Arc<dyn UserRepository> = Arc::new(SqliteUserRepository::new(pool));
        let service = Arc::new(QuotaService::new(repo, users, ctx.audit.clone()));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "per-user / per-site resource quotas".to_string(),
                sql: crate::migrations::QUOTAS_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<QuotaService> {
        self.service.clone()
    }
}

impl Module for QuotasModule {
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

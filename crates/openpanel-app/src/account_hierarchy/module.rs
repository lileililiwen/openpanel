//! Account-hierarchy module: composition root for the bounded
//! context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::UserRepository;

use crate::{
    account_hierarchy::{HierarchyService, SqliteHierarchyRepository},
    identity::repo::SqliteUserRepository,
};

/// Stable module name used in migration bookkeeping.
pub const MODULE_NAME: &str = "account-hierarchy";

/// Account-hierarchy module wires the SQLite adapter, the
/// application service, and the migration.
pub struct AccountHierarchyModule {
    service: Arc<HierarchyService>,
    migrations: Vec<Migration>,
}

impl AccountHierarchyModule {
    /// Build the module from the panel application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteHierarchyRepository::new(pool.clone()));
        let users: Arc<dyn UserRepository> = Arc::new(SqliteUserRepository::new(pool));
        let service = Arc::new(HierarchyService::new(repo, users, ctx.audit.clone()));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "account relationships, quota pools, and pool claims".to_string(),
                sql: crate::migrations::ACCOUNT_HIERARCHY_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<HierarchyService> {
        self.service.clone()
    }
}

impl Module for AccountHierarchyModule {
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

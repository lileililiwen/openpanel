//! Hosting-plans module: composition root for the bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::UserRepository;

use crate::{
    hosting_plans::{HostingPlansService, SqliteHostingPlanRepository},
    identity::repo::SqliteUserRepository,
};

/// Stable module name used in migration bookkeeping.
pub const MODULE_NAME: &str = "hosting-plans";

/// Hosting-plans module wires the SQLite adapter, the application
/// service, and the migration.
pub struct HostingPlansModule {
    service: Arc<HostingPlansService>,
    migrations: Vec<Migration>,
}

impl HostingPlansModule {
    /// Build the module from the panel application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteHostingPlanRepository::new(pool.clone()));
        let users: Arc<dyn UserRepository> = Arc::new(SqliteUserRepository::new(pool));
        let service = Arc::new(HostingPlansService::new(repo, users, ctx.audit.clone()));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "hosting plans and assignments".to_string(),
                sql: crate::migrations::HOSTING_PLANS_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<HostingPlansService> {
        self.service.clone()
    }
}

impl Module for HostingPlansModule {
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

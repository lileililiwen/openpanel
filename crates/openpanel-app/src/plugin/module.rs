//! `plugin-extension-framework` module composition.

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};

use crate::plugin::{repo::SqlitePluginRegistry, service::PluginService};

/// Stable module identifier.
pub const MODULE_NAME: &str = "plugin-extension-framework";

/// Plugin extension framework module: signed manifests, capability
/// gating, lifecycle service, and SQLite repository.
pub struct PluginModule {
    service: Arc<PluginService>,
    migrations: Vec<Migration>,
}

impl PluginModule {
    /// Build the module from the shared `AppContext` and an audit sink.
    pub async fn new(ctx: &AppContext, audit: Arc<dyn AuditService>) -> Self {
        let pool = ctx.db.pool().await;
        let registry = Arc::new(SqlitePluginRegistry::new(pool));
        let service = Arc::new(PluginService::new(registry, audit));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "plugin registry initial schema".into(),
                sql: crate::migrations::PLUGIN_V001.into(),
            }],
        }
    }

    /// Access the lifecycle service.
    pub fn service(&self) -> Arc<PluginService> {
        self.service.clone()
    }
}

impl Module for PluginModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

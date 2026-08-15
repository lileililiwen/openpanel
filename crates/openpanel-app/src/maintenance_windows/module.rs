//! Maintenance windows composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{MaintenanceEnforcer, SqliteMaintenanceRepository};

/// Stable maintenance-windows module name.
pub const MODULE_NAME: &str = "maintenance_windows";

/// Maintenance windows bounded-context composition root.
pub struct MaintenanceWindowsModule {
    repo: Arc<SqliteMaintenanceRepository>,
    enforcer: Arc<MaintenanceEnforcer>,
    migrations: Vec<Migration>,
}

impl MaintenanceWindowsModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteMaintenanceRepository::new(pool));
        let enforcer = Arc::new(MaintenanceEnforcer::new(repo.clone(), ctx.audit.clone()));
        Self {
            repo,
            enforcer,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "maintenance windows + single-use overrides".to_owned(),
                sql: crate::migrations::MAINTENANCE_WINDOWS_V001.to_owned(),
            }],
        }
    }

    /// Shared enforcer.
    pub fn enforcer(&self) -> Arc<MaintenanceEnforcer> {
        self.enforcer.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteMaintenanceRepository> {
        self.repo.clone()
    }
}

impl Module for MaintenanceWindowsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}
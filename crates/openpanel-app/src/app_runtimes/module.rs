//! Non-PHP runtime composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{
    ReverseProxyLayer, RuntimeService, SqliteRuntimeRepository, SupervisorUnitBuilder,
    env_service::RuntimeEnvService,
};

/// Stable app-runtimes module name.
pub const MODULE_NAME: &str = "app_runtimes";

/// Non-PHP runtime bounded-context composition root.
pub struct AppRuntimesModule {
    repo: Arc<SqliteRuntimeRepository>,
    service: Arc<RuntimeService>,
    env: Arc<RuntimeEnvService>,
    migrations: Vec<Migration>,
}

impl AppRuntimesModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext, master_key: [u8; 32]) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteRuntimeRepository::new(pool.clone()));
        let env = Arc::new(RuntimeEnvService::new(
            pool.clone(),
            ctx.audit.clone(),
            master_key,
        ));
        let service = Arc::new(RuntimeService::new(
            repo.clone(),
            ctx.audit.clone(),
            SupervisorUnitBuilder::new(),
            ReverseProxyLayer::new(),
        ));
        Self {
            repo,
            service,
            env,
            migrations: vec![
                Migration {
                    module: MODULE_NAME,
                    version: "001".to_owned(),
                    description: "non-PHP runtime: per-site runtime config".to_owned(),
                    sql: crate::migrations::APP_RUNTIMES_V001.to_owned(),
                },
                Migration {
                    module: MODULE_NAME,
                    version: "002".to_owned(),
                    description: "runtime environment sets".to_owned(),
                    sql: crate::migrations::APP_RUNTIMES_V002.to_owned(),
                },
            ],
        }
    }

    /// Shared environment service.
    pub fn env(&self) -> Arc<RuntimeEnvService> {
        self.env.clone()
    }

    /// Shared service.
    pub fn service(&self) -> Arc<RuntimeService> {
        self.service.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteRuntimeRepository> {
        self.repo.clone()
    }
}

impl Module for AppRuntimesModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

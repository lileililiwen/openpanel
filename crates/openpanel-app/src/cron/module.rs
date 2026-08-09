//! Cron bounded-context composition.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AppContext, Migration, Module};

use super::service::CronService;

/// Stable module identifier.
pub const MODULE_NAME: &str = "cron";

/// Cron module with persistent service and schema.
pub struct CronModule {
    service: Arc<CronService>,
    migrations: Vec<Migration>,
}

impl CronModule {
    /// Build a production module restricted to hosted-site storage.
    pub async fn new(ctx: &AppContext) -> Self {
        Self::with_roots(ctx, vec![PathBuf::from("/var/www")]).await
    }

    /// Build with explicit allowed roots for sandboxed deployments and tests.
    pub async fn with_roots(ctx: &AppContext, roots: Vec<PathBuf>) -> Self {
        let service = Arc::new(CronService::new(
            ctx.db.pool().await,
            roots,
            ctx.audit.clone(),
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "cron jobs and run history".into(),
                sql: crate::migrations::CRON_V001.into(),
            }],
        }
    }

    /// Shared service handle.
    pub fn service(&self) -> Arc<CronService> {
        self.service.clone()
    }
}

impl Module for CronModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn background_tasks(
        &self,
        _ctx: &AppContext,
    ) -> Vec<Box<dyn openpanel_core::jobs::BackgroundTask>> {
        vec![Box::new(super::task::CronSchedulerTask::new(
            self.service.clone(),
            15,
            30,
        ))]
    }
}

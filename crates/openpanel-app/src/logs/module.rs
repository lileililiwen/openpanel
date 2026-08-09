//! Logs bounded-context composition.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AppContext, Migration, Module};

use super::LogService;

/// Stable module name.
pub const MODULE_NAME: &str = "logs";

/// Registered logs service and aggregation schema.
pub struct LogsModule {
    service: Arc<LogService>,
    migrations: Vec<Migration>,
}

impl LogsModule {
    /// Build with the production nginx-log root.
    pub async fn new(ctx: &AppContext) -> Self {
        Self::with_root(ctx, PathBuf::from("/var/log/openpanel")).await
    }

    /// Build with an explicit sandbox root.
    pub async fn with_root(ctx: &AppContext, root: PathBuf) -> Self {
        Self {
            service: Arc::new(LogService::for_root(
                ctx.db.pool().await,
                root,
                ctx.audit.clone(),
            )),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "traffic aggregates and source offsets".into(),
                sql: crate::migrations::LOGS_V001.into(),
            }],
        }
    }

    /// Shared service handle.
    pub fn service(&self) -> Arc<LogService> {
        self.service.clone()
    }
}

impl Module for LogsModule {
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
        vec![Box::new(super::task::LogAggregationTask {
            service: self.service.clone(),
        })]
    }
}

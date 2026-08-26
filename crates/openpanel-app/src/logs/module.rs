//! Logs bounded-context composition.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AppContext, Migration, Module};

use super::{LogService, rotation_service::LogRotationService};

/// Stable module name.
pub const MODULE_NAME: &str = "logs";

/// Registered logs service and aggregation schema.
pub struct LogsModule {
    service: Arc<LogService>,
    rotation: Arc<LogRotationService>,
    migrations: Vec<Migration>,
}

impl LogsModule {
    /// Build with the production nginx-log root.
    pub async fn new(ctx: &AppContext) -> Self {
        Self::with_root(ctx, PathBuf::from("/var/log/openpanel")).await
    }

    /// Build with an explicit sandbox root.
    pub async fn with_root(ctx: &AppContext, root: PathBuf) -> Self {
        let drop_in_root = std::env::var("OPENPANEL__LOGS__DROP_IN_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/etc/logrotate.d"));
        Self::with_roots(ctx, root, drop_in_root).await
    }

    /// Build with explicit log and logrotate drop-in roots.
    pub async fn with_roots(ctx: &AppContext, root: PathBuf, drop_in_root: PathBuf) -> Self {
        let pool = ctx.db.pool().await;
        let rotation = Arc::new(LogRotationService::new(
            pool.clone(),
            ctx.audit.clone(),
            drop_in_root,
        ));
        Self {
            service: Arc::new(LogService::for_root(pool, root, ctx.audit.clone())),
            rotation,
            migrations: vec![
                Migration {
                    module: MODULE_NAME,
                    version: "001".into(),
                    description: "traffic aggregates and source offsets".into(),
                    sql: crate::migrations::LOGS_V001.into(),
                },
                Migration {
                    module: MODULE_NAME,
                    version: "002".into(),
                    description: "log rotation policies".into(),
                    sql: crate::migrations::LOGS_V002.into(),
                },
            ],
        }
    }

    /// Shared rotation-policy service handle.
    pub fn rotation(&self) -> Arc<LogRotationService> {
        self.rotation.clone()
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

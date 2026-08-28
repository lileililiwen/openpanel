//! Backup module composition.

use std::{path::PathBuf, sync::Arc};

use openpanel_core::{AppContext, Migration, Module};

use super::{DrillService, service::BackupService};

/// Stable module identifier.
pub const MODULE_NAME: &str = "backups";

/// Backup and restore bounded-context module.
pub struct BackupsModule {
    service: Arc<BackupService>,
    drill_service: Arc<DrillService>,
    migrations: Vec<Migration>,
}
impl BackupsModule {
    /// Build using `/var/lib/openpanel/backups`.
    pub async fn new(
        ctx: &AppContext,
        master_key: Option<[u8; 32]>,
        cron: Option<(Arc<crate::CronService>, PathBuf)>,
    ) -> Self {
        Self::with_root(
            ctx,
            PathBuf::from("/var/lib/openpanel/backups"),
            master_key,
            cron,
        )
        .await
    }

    /// Build with a sandboxed local destination.
    pub async fn with_root(
        ctx: &AppContext,
        root: PathBuf,
        master_key: Option<[u8; 32]>,
        cron: Option<(Arc<crate::CronService>, PathBuf)>,
    ) -> Self {
        let service = Arc::new(BackupService::new(
            ctx.db.pool().await,
            root,
            ctx.audit.clone(),
            master_key,
            cron,
        ));
        let drill_service = Arc::new(DrillService::new(
            ctx.db.pool().await,
            service.clone(),
            ctx.audit.clone(),
            std::env::var("OPENPANEL_DRILL_MYSQL_BIN")
                .ok()
                .or_else(|| Some("/usr/bin/mysql".to_string())),
        ));
        Self {
            service,
            drill_service,
            migrations: vec![
                Migration {
                    module: MODULE_NAME,
                    version: "001".into(),
                    description: "backup plans runs and restore jobs".into(),
                    sql: crate::migrations::BACKUPS_V001.into(),
                },
                Migration {
                    module: MODULE_NAME,
                    version: "002".into(),
                    description: "backup restore drills".into(),
                    sql: crate::migrations::BACKUPS_V002.into(),
                },
            ],
        }
    }

    /// Shared service handle.
    pub fn service(&self) -> Arc<BackupService> {
        self.service.clone()
    }

    /// Shared drill service handle.
    pub fn drill_service(&self) -> Arc<DrillService> {
        self.drill_service.clone()
    }
}
impl Module for BackupsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

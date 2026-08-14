//! OS update management composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{
    AptPackageManager, OsUpdateApplier, OsUpdateLister, PackageManager, SqliteOsUpdateRepository,
    UnattendedConfig,
};

/// Stable OS update management module name.
pub const MODULE_NAME: &str = "os_updates";

/// OS update management bounded-context composition root.
pub struct OsUpdateModule {
    repo: Arc<SqliteOsUpdateRepository>,
    lister: Arc<OsUpdateLister>,
    applier: Arc<OsUpdateApplier>,
    unattended: Arc<UnattendedConfig>,
    migrations: Vec<Migration>,
}

impl OsUpdateModule {
    /// Compose the bounded context with the default `apt` package
    /// manager and the default unattended config path.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteOsUpdateRepository::new(pool));
        let package_manager: Arc<dyn PackageManager> = Arc::new(AptPackageManager::new());
        let lister = Arc::new(OsUpdateLister::new(package_manager.clone()));
        let applier = Arc::new(OsUpdateApplier::new(
            repo.clone(),
            ctx.audit.clone(),
            package_manager,
        ));
        let unattended = Arc::new(UnattendedConfig::new(UnattendedConfig::default_path()));
        Self {
            repo,
            lister,
            applier,
            unattended,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "OS update policy and apply history".to_owned(),
                sql: crate::migrations::OS_UPDATES_V001.to_owned(),
            }],
        }
    }

    /// Shared lister.
    pub fn lister(&self) -> Arc<OsUpdateLister> {
        self.lister.clone()
    }

    /// Shared applier.
    pub fn applier(&self) -> Arc<OsUpdateApplier> {
        self.applier.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteOsUpdateRepository> {
        self.repo.clone()
    }

    /// Shared unattended-config writer.
    pub fn unattended(&self) -> Arc<UnattendedConfig> {
        self.unattended.clone()
    }
}

impl Module for OsUpdateModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

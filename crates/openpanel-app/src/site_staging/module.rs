//! `site-staging` module composition: wires the service, repositories,
//! lock table, filesystem layer, and migrations into the shared
//! [`ModuleRegistry`](openpanel_core::Module).

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};
use openpanel_domain::{
    PromotionRepository, SiteRepository, StagingSlotRepository, StagingSnapshotRepository,
};
use openpanel_site_staging_files::StagingFilesystemLayer;

use crate::site_staging::{
    repo::{
        SqlitePromotionRepository, SqliteStagingLockTable, SqliteStagingSlotRepository,
        SqliteStagingSnapshotRepository,
    },
    service::StagingService,
};

/// Re-export the files module so the binary entry points can
/// construct production filesystem layers.
pub mod openpanel_site_staging_files {
    pub use crate::site_staging::files::StagingFilesystemLayer;
}

/// Stable module identifier.
pub const MODULE_NAME: &str = "site-staging";

/// Site-staging module.
pub struct SiteStagingModule {
    service: Arc<StagingService>,
    migrations: Vec<Migration>,
}

impl SiteStagingModule {
    /// Build the module from the shared `AppContext`, the sites
    /// repository, and a filesystem layer.
    pub async fn new(
        ctx: &AppContext,
        sites: Arc<dyn SiteRepository>,
        fs: Arc<dyn StagingFilesystemLayer>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let slots: Arc<dyn StagingSlotRepository> =
            Arc::new(SqliteStagingSlotRepository::new(pool.clone()));
        let snapshots: Arc<dyn StagingSnapshotRepository> =
            Arc::new(SqliteStagingSnapshotRepository::new(pool.clone()));
        let promotions: Arc<dyn PromotionRepository> =
            Arc::new(SqlitePromotionRepository::new(pool.clone()));
        let locks = SqliteStagingLockTable::new(pool.clone());
        let service = Arc::new(StagingService::new(
            slots, snapshots, promotions, sites, fs, locks, audit,
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "site-staging initial schema".into(),
                sql: crate::migrations::SITE_STAGING_V001.into(),
            }],
        }
    }

    /// Shared service handle.
    pub fn service(&self) -> Arc<StagingService> {
        self.service.clone()
    }
}

impl Module for SiteStagingModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

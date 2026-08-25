//! Site HTTP-controls composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{SiteRepository, site_http_controls::SiteHttpRepository};

use super::{NginxHttpControlsApplier, SiteHttpService, SqliteSiteHttpRepository};
use crate::sites::{nginx::NginxConfigGenerator, repo::SqliteSiteRepository};

/// Stable module name.
pub const MODULE_NAME: &str = "site_http_controls";

/// Site HTTP-controls bounded-context composition root.
pub struct SiteHttpControlsModule {
    service: Arc<SiteHttpService>,
    migrations: Vec<Migration>,
}

impl SiteHttpControlsModule {
    /// Compose SQLite persistence and nginx application ports.
    pub async fn new(
        ctx: &AppContext,
        generator: NginxConfigGenerator,
        auth_dir: std::path::PathBuf,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn SiteHttpRepository> =
            Arc::new(SqliteSiteHttpRepository::new(pool.clone()));
        let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool));
        let service = Arc::new(SiteHttpService::new(
            repo,
            sites,
            ctx.audit.clone(),
            Arc::new(NginxHttpControlsApplier::new(generator)),
            auth_dir,
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "per-site HTTP controls documents".to_owned(),
                sql: crate::migrations::SITE_HTTP_CONTROLS_V001.to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<SiteHttpService> {
        self.service.clone()
    }
}

impl Module for SiteHttpControlsModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

//! Sites module: registers the SitesService, migrations.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::sites::nginx::{NginxConfigGenerator, NginxPaths};
use crate::sites::repo::SqliteSiteRepository;
use crate::sites::service::SitesService;

pub const MODULE_NAME: &str = "sites";

pub struct SitesModule {
    service: Arc<SitesService>,
    migrations: Vec<Migration>,
    generator: NginxConfigGenerator,
}

impl SitesModule {
    pub async fn new(ctx: &AppContext) -> Self {
        let paths = NginxPaths::detect();
        Self::with_paths(ctx, paths).await
    }

    /// Construct with custom nginx paths. Useful for tests that need to
    /// write configs to a sandbox directory instead of `/etc/nginx/`.
    pub async fn with_paths(ctx: &AppContext, paths: NginxPaths) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn openpanel_domain::SiteRepository> =
            Arc::new(SqliteSiteRepository::new(pool));
        let generator = NginxConfigGenerator::new(paths);
        let service = Arc::new(SitesService::new(
            repo,
            ctx.audit.clone(),
            generator.clone(),
        ));
        let migrations = vec![Migration {
            module: MODULE_NAME,
            version: "001".to_string(),
            description: "sites initial schema".to_string(),
            sql: crate::migrations::SITES_V001.to_string(),
        }];
        Self {
            service,
            migrations,
            generator,
        }
    }

    pub fn service(&self) -> Arc<SitesService> {
        self.service.clone()
    }

    pub fn generator(&self) -> &NginxConfigGenerator {
        &self.generator
    }
}

impl Module for SitesModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

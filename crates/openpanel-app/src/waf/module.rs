//! WAF composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::{SiteRepository, WafRepository};

use super::{NginxWafConfigApplier, SqliteWafRepository, WafService};
use crate::sites::{nginx::NginxConfigGenerator, repo::SqliteSiteRepository};

/// Stable WAF module name.
pub const MODULE_NAME: &str = "waf";

/// WAF bounded-context composition root.
pub struct WafModule {
    service: Arc<WafService>,
    migrations: Vec<Migration>,
}

impl WafModule {
    /// Compose SQLite persistence and nginx application ports.
    pub async fn new(ctx: &AppContext, generator: NginxConfigGenerator) -> Self {
        let pool = ctx.db.pool().await;
        let repo: Arc<dyn WafRepository> = Arc::new(SqliteWafRepository::new(pool.clone()));
        let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool));
        let service = Arc::new(WafService::new(
            repo,
            sites,
            ctx.audit.clone(),
            Arc::new(NginxWafConfigApplier::new(generator)),
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "per-site WAF rules and hit metrics".to_owned(),
                sql: crate::migrations::WAF_V001.to_owned(),
            }],
        }
    }

    /// Shared WAF service.
    pub fn service(&self) -> Arc<WafService> {
        self.service.clone()
    }
}

impl Module for WafModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

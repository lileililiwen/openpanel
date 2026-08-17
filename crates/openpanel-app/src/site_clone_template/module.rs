//! Site clone and template export module: composition root for the
//! bounded context. The module registers the SQLite repository and
//! the migration; the `SiteCloneService` itself is constructed
//! inline in the API/CLI composition root where the `SitesService`
//! and master key are available.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use crate::site_clone_template::SqliteSiteCloneTemplateRepository;

/// Stable module name.
pub const MODULE_NAME: &str = "site_clone_template";

/// Site clone + template export module.
pub struct SiteCloneTemplateModule {
    repo: Arc<SqliteSiteCloneTemplateRepository>,
    migrations: Vec<Migration>,
}

impl SiteCloneTemplateModule {
    /// Build the module from the application context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteSiteCloneTemplateRepository::new(pool));
        Self {
            repo,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "site_templates, clone_plans, clone_runs, anonymisation_tokens"
                    .to_string(),
                sql: crate::migrations::SITE_CLONE_TEMPLATE_V001.to_string(),
            }],
        }
    }

    /// The shared SQLite repository (the API/CLI composition root
    /// uses this handle to construct a `SiteCloneService`).
    pub fn repo(&self) -> Arc<SqliteSiteCloneTemplateRepository> {
        self.repo.clone()
    }
}

impl Module for SiteCloneTemplateModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
}

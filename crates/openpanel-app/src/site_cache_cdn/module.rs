//! Site cache and CDN module: composition root for the bounded context.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};
use openpanel_domain::CdnKind;

use crate::site_cache_cdn::{
    CdnAdapterRegistry, GenericHttpAdapter, SiteCacheService, SqliteSiteCacheCdnRepository,
};

/// Stable module name.
pub const MODULE_NAME: &str = "site_cache_cdn";

/// Site cache and CDN module wires the SQLite adapter, the service,
/// the generic-HTTP adapter, and the migration.
pub struct SiteCacheCdnModule {
    service: Arc<SiteCacheService>,
    migrations: Vec<Migration>,
}

impl SiteCacheCdnModule {
    /// Build the module from the panel application context. The
    /// generic-HTTP adapter is registered for `generic_http`;
    /// Cloudflare / CloudFront adapters register in follow-on
    /// changes when their SDK plumbing is available.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteSiteCacheCdnRepository::new(pool));
        let mut registry = CdnAdapterRegistry::new();
        registry.register(CdnKind::GenericHttp, GenericHttpAdapter::new(""));
        let service = Arc::new(SiteCacheService::new(
            repo,
            ctx.audit.clone(),
            Arc::new(registry),
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_string(),
                description: "site cache policies, CDN integrations, purge log".to_string(),
                sql: crate::migrations::SITE_CACHE_CDN_V001.to_string(),
            }],
        }
    }

    /// Return the shared service handle.
    pub fn service(&self) -> Arc<SiteCacheService> {
        self.service.clone()
    }
}

impl Module for SiteCacheCdnModule {
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

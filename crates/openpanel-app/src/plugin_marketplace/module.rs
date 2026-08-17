//! Plugin marketplace module composition.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{cache::SqliteCatalogCache, client::MarketplaceClient, service::MarketplaceService};
use crate::plugin::service::PluginService;

/// Stable module identifier.
pub const MODULE_NAME: &str = "plugin-marketplace";

/// Plugin marketplace module: remote catalog discovery, publisher
/// CA verification, ratings/metadata, and delegated install.
pub struct PluginMarketplaceModule {
    service: Arc<MarketplaceService>,
    migrations: Vec<Migration>,
}

impl PluginMarketplaceModule {
    /// Build the module from the shared `AppContext`, a transport
    /// client, and the base plugin service. MUST be called from
    /// within an active Tokio runtime (the test support and
    /// composition root both satisfy this).
    pub async fn new(
        ctx: &AppContext,
        client: Arc<dyn MarketplaceClient>,
        plugins: Arc<PluginService>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        let cache: Arc<dyn openpanel_domain::CatalogCache> =
            Arc::new(SqliteCatalogCache::new(pool));
        let ca = openpanel_domain::MarketplaceCa::empty();
        let service = Arc::new(MarketplaceService::new(
            client,
            cache,
            ca,
            plugins,
            ctx.audit.clone(),
        ));
        Self {
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "marketplace catalog cache initial schema".into(),
                sql: crate::migrations::PLUGIN_MARKETPLACE_V001.into(),
            }],
        }
    }

    /// Access the marketplace service.
    pub fn service(&self) -> Arc<MarketplaceService> {
        self.service.clone()
    }
}

impl Module for PluginMarketplaceModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

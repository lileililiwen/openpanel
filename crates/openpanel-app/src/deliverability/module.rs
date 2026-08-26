//! Email deliverability bounded-context composition.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::DeliverabilityService;

/// Stable module name.
pub const MODULE_NAME: &str = "deliverability";

/// Deliverability module: migrations + shared service.
pub struct DeliverabilityModule {
    service: Arc<DeliverabilityService>,
    migrations: Vec<Migration>,
}

impl DeliverabilityModule {
    /// Compose with the resolver port and configured zones.
    pub async fn new(
        ctx: &AppContext,
        resolver: Arc<dyn openpanel_domain::deliverability::ResolverPort>,
        zones: Vec<openpanel_domain::deliverability::BlocklistZone>,
    ) -> Self {
        let pool = ctx.db.pool().await;
        Self {
            service: Arc::new(DeliverabilityService::new(
                pool,
                ctx.audit.clone(),
                resolver,
                zones,
            )),
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".into(),
                description: "blocklist listings and DMARC source stats".into(),
                sql: crate::migrations::DELIVERABILITY_V001.into(),
            }],
        }
    }

    /// Shared service handle.
    pub fn service(&self) -> Arc<DeliverabilityService> {
        self.service.clone()
    }
}

impl Module for DeliverabilityModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

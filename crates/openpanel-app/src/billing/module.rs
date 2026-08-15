//! Reseller billing composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{
    BillingService, ChargebackEngine, SqliteBillingRepository, UsageExporter, WebhookRelay,
};

/// Stable billing module name.
pub const MODULE_NAME: &str = "billing";

/// Reseller billing bounded-context composition root.
pub struct BillingModule {
    repo: Arc<SqliteBillingRepository>,
    service: Arc<BillingService>,
    migrations: Vec<Migration>,
}

impl BillingModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteBillingRepository::new(pool));
        let exporter = UsageExporter::new(repo.clone());
        let engine = ChargebackEngine::new(repo.clone(), ctx.audit.clone());
        let relay = WebhookRelay::new(repo.clone(), ctx.audit.clone());
        let service = Arc::new(BillingService::new(exporter, engine, relay));
        Self {
            repo,
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "reseller billing: usage, chargeback, integrations".to_owned(),
                sql: crate::migrations::BILLING_V001.to_owned(),
            }],
        }
    }

    /// Shared billing service.
    pub fn service(&self) -> Arc<BillingService> {
        self.service.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteBillingRepository> {
        self.repo.clone()
    }
}

impl Module for BillingModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

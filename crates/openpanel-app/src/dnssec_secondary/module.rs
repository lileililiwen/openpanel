//! DNSSEC + secondary DNS composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, Migration, Module};

use super::{DnsSecService, RecordingRegistrar, Registrar, SqliteDnsSecRepository};

/// Stable DNSSEC + secondary DNS module name.
pub const MODULE_NAME: &str = "dnssec_secondary";

/// DNSSEC + secondary DNS bounded-context composition root.
pub struct DnsSecSecondaryModule {
    repo: Arc<SqliteDnsSecRepository>,
    service: Arc<DnsSecService>,
    migrations: Vec<Migration>,
}

impl DnsSecSecondaryModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteDnsSecRepository::new(pool));
        let registrar: Arc<dyn Registrar> = Arc::new(RecordingRegistrar::new());
        let service = Arc::new(DnsSecService::new(
            repo.clone(),
            ctx.audit.clone(),
            registrar,
        ));
        Self {
            repo,
            service,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "DNSSEC + secondary DNS: policies, keys, secondaries, glue, DS"
                    .to_owned(),
                sql: crate::migrations::DNSSEC_SECONDARY_V001.to_owned(),
            }],
        }
    }

    /// Shared service.
    pub fn service(&self) -> Arc<DnsSecService> {
        self.service.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteDnsSecRepository> {
        self.repo.clone()
    }
}

impl Module for DnsSecSecondaryModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

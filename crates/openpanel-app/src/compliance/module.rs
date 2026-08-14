//! Compliance composition module and migration registration.

use std::sync::Arc;

use openpanel_core::{AppContext, AuditService, Migration, Module};

use super::{
    AuditRetentionService, GdprExporter, HardeningWizard, SqliteComplianceRepository,
};

/// Stable compliance module name.
pub const MODULE_NAME: &str = "compliance";

/// Default CIS profile shipped with the panel.
pub fn default_profile() -> &'static str {
    super::service::DEFAULT_PROFILE
}

/// Compliance bounded-context composition root.
pub struct ComplianceModule {
    repo: Arc<SqliteComplianceRepository>,
    wizard: Arc<HardeningWizard>,
    retention: Arc<AuditRetentionService>,
    exporter: Arc<GdprExporter>,
    migrations: Vec<Migration>,
}

impl ComplianceModule {
    /// Compose the bounded context.
    pub async fn new(ctx: &AppContext) -> Self {
        let pool = ctx.db.pool().await;
        let repo = Arc::new(SqliteComplianceRepository::new(pool));
        let wizard = Arc::new(HardeningWizard::new(repo.clone(), ctx.audit.clone()));
        let retention = Arc::new(AuditRetentionService::new(
            repo.clone(),
            ctx.audit.clone(),
        ));
        let exporter = Arc::new(GdprExporter::new(repo.clone(), ctx.audit.clone()));
        Self {
            repo,
            wizard,
            retention,
            exporter,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "CIS hardening runs, audit retention, GDPR exports".to_owned(),
                sql: crate::migrations::COMPLIANCE_V001.to_owned(),
            }],
        }
    }

    /// Construct a module with a custom audit service (test wiring).
    pub async fn with_audit(audit: Arc<dyn AuditService>, pool: sqlx::SqlitePool) -> Self {
        let repo = Arc::new(SqliteComplianceRepository::new(pool));
        let wizard = Arc::new(HardeningWizard::new(repo.clone(), audit.clone()));
        let retention = Arc::new(AuditRetentionService::new(repo.clone(), audit.clone()));
        let exporter = Arc::new(GdprExporter::new(repo.clone(), audit));
        Self {
            repo,
            wizard,
            retention,
            exporter,
            migrations: vec![Migration {
                module: MODULE_NAME,
                version: "001".to_owned(),
                description: "CIS hardening runs, audit retention, GDPR exports".to_owned(),
                sql: crate::migrations::COMPLIANCE_V001.to_owned(),
            }],
        }
    }

    /// Shared wizard.
    pub fn wizard(&self) -> Arc<HardeningWizard> {
        self.wizard.clone()
    }

    /// Shared retention service.
    pub fn retention(&self) -> Arc<AuditRetentionService> {
        self.retention.clone()
    }

    /// Shared exporter.
    pub fn exporter(&self) -> Arc<GdprExporter> {
        self.exporter.clone()
    }

    /// Shared repository.
    pub fn repo(&self) -> Arc<SqliteComplianceRepository> {
        self.repo.clone()
    }
}

impl Module for ComplianceModule {
    fn name(&self) -> &'static str {
        MODULE_NAME
    }

    fn migrations(&self) -> Vec<Migration> {
        self.migrations.clone()
    }
}

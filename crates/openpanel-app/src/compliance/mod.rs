//! Compliance application layer: SQLite repository, hardening wizard,
//! retention service, and GDPR exporter.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{ComplianceModule, MODULE_NAME, default_profile};
pub use repo::SqliteComplianceRepository;
pub use service::{
    AuditPurge, AuditRetentionService, GdprExporter, GdprSourceApiToken, GdprSourceDatabase,
    GdprSourceMailbox, GdprSourceSite, GdprSources, HardeningOutcome, HardeningWizard,
    InMemoryRuleExecutor, RollbackOutcome, RuleExecutor, RuleImages,
};

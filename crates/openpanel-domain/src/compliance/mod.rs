//! Compliance bounded context: CIS hardening runs, audit retention
//! policy, and GDPR data export with secret redaction.
//!
//! Hardening runs are **reversible**: each applied rule records its
//! pre-image so an Admin can roll back to the prior state. The GDPR
//! export never embeds raw secrets; every database password, API
//! token, or private key is replaced with a redaction marker.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the compliance bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComplianceError {
    /// The caller is not authorised (Admin role required).
    #[error("forbidden")]
    Forbidden,
    /// The requested user does not exist.
    #[error("user not found: {0}")]
    UserNotFound(Uuid),
    /// The hardening run does not exist.
    #[error("hardening run not found: {0}")]
    HardeningRunNotFound(Uuid),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
    /// Hardening execution failed.
    #[error("hardening failed: {0}")]
    Hardening(String),
}

impl From<ComplianceError> for RepoError {
    fn from(error: ComplianceError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for ComplianceError {
    fn from(error: RepoError) -> Self {
        ComplianceError::Persistence(error.0)
    }
}

/// One applied rule within a hardening run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardeningRule {
    /// Stable rule id (CIS reference like `5.2.1`).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Pre-image snapshot for rollback.
    pub pre_image: serde_json::Value,
    /// Post-image after apply.
    pub post_image: serde_json::Value,
    /// Result of the apply step.
    pub outcome: RuleOutcome,
}

impl HardeningRule {
    /// Build a successful rule with a pre/post image.
    pub fn applied(
        id: impl Into<String>,
        title: impl Into<String>,
        pre: serde_json::Value,
        post: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            pre_image: pre,
            post_image: post,
            outcome: RuleOutcome::Applied,
        }
    }

    /// Mark the rule as skipped.
    pub fn skipped(
        id: impl Into<String>,
        title: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            pre_image: serde_json::Value::Null,
            post_image: serde_json::Value::Null,
            outcome: RuleOutcome::Skipped(reason.into()),
        }
    }

    /// Mark the rule as failed.
    pub fn failed(
        id: impl Into<String>,
        title: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            pre_image: serde_json::Value::Null,
            post_image: serde_json::Value::Null,
            outcome: RuleOutcome::Failed(error.into()),
        }
    }
}

/// Outcome of a single rule application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleOutcome {
    /// Rule was applied successfully.
    Applied,
    /// Rule was skipped (e.g. condition not met).
    Skipped(String),
    /// Rule failed (caller decides whether to abort).
    Failed(String),
}

impl RuleOutcome {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Skipped(_) => "skipped",
            Self::Failed(_) => "failed",
        }
    }
}

/// A complete hardening run: profile, rules, and a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardeningRun {
    /// Stable run id.
    pub id: Uuid,
    /// CIS profile name (e.g. `cis-debian-12`).
    pub profile: String,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// When the run completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Principal that initiated the run.
    pub initiated_by: Uuid,
    /// Rule-by-rule outcome.
    pub rules: Vec<HardeningRule>,
    /// True if any rule failed.
    pub has_failures: bool,
}

impl HardeningRun {
    /// Create a new run with no rules yet.
    pub fn new(profile: impl Into<String>, initiated_by: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            profile: profile.into(),
            started_at: Utc::now(),
            completed_at: None,
            initiated_by,
            rules: Vec::new(),
            has_failures: false,
        }
    }

    /// Append a rule and recompute failure status.
    pub fn record(&mut self, rule: HardeningRule) {
        if matches!(rule.outcome, RuleOutcome::Failed(_)) {
            self.has_failures = true;
        }
        self.rules.push(rule);
    }

    /// Mark the run as complete.
    pub fn finish(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    /// Roll back every `Applied` rule. Returns the rules that were
    /// reverted, in order.
    pub fn rollback_targets(&self) -> impl Iterator<Item = &HardeningRule> {
        self.rules
            .iter()
            .filter(|r| matches!(r.outcome, RuleOutcome::Applied))
    }
}

/// Audit-log retention policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRetentionPolicy {
    /// Retention in days. Records older than this are purged.
    pub ttl_days: u32,
    /// Whether to export records before purging.
    pub export_before_purge: bool,
    /// When the policy was last changed.
    pub updated_at: DateTime<Utc>,
    /// Principal that last changed the policy.
    pub updated_by: Uuid,
}

impl AuditRetentionPolicy {
    /// Conservative default: 365 days, no scheduled export.
    pub fn default(updated_by: Uuid) -> Self {
        Self {
            ttl_days: 365,
            export_before_purge: false,
            updated_at: Utc::now(),
            updated_by,
        }
    }

    /// Validate the TTL is within the supported range.
    pub fn validate(&self) -> Result<(), ComplianceError> {
        if self.ttl_days < 1 || self.ttl_days > 3650 {
            return Err(ComplianceError::Hardening(
                "ttl_days must be between 1 and 3650".into(),
            ));
        }
        Ok(())
    }
}

/// A GDPR data export with secret redaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GdprExport {
    /// Stable export id.
    pub id: Uuid,
    /// User whose PII is being exported.
    pub user_id: Uuid,
    /// Aggregated PII payload (redacted).
    pub payload: GdprExportPayload,
    /// When the export was generated.
    pub generated_at: DateTime<Utc>,
}

/// Aggregated PII payload across sites, mail, and databases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GdprExportPayload {
    /// Sites owned by the user.
    pub sites: Vec<GdprSiteRecord>,
    /// Mailboxes and aliases.
    pub mail: Vec<GdprMailRecord>,
    /// Databases owned by the user.
    pub databases: Vec<GdprDatabaseRecord>,
    /// Personal API tokens.
    pub api_tokens: Vec<GdprApiTokenRecord>,
}

/// Site record in a GDPR export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GdprSiteRecord {
    /// Site id.
    pub id: Uuid,
    /// Primary domain.
    pub domain: String,
    /// Whitespace-separated aliases.
    pub aliases: String,
    /// Owning user.
    pub owner_id: Uuid,
}

/// Mail record in a GDPR export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GdprMailRecord {
    /// Mailbox address.
    pub address: String,
    /// Mailbox display name.
    pub display_name: String,
    /// Optional quota in bytes.
    pub quota_bytes: Option<u64>,
}

/// Database record in a GDPR export. The password is replaced with
/// the redaction marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GdprDatabaseRecord {
    /// Database id.
    pub id: Uuid,
    /// Database name.
    pub name: String,
    /// Owning user.
    pub owner_id: Uuid,
    /// Redaction marker (never the real password).
    pub password: String,
}

/// API token record. The token credential is always redacted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GdprApiTokenRecord {
    /// Token id.
    pub id: Uuid,
    /// Stable token name.
    pub name: String,
    /// Scopes the token was issued for.
    pub scopes: Vec<String>,
    /// Redaction marker (never the raw token).
    pub token: String,
}

/// Sentinel placeholder for redacted secrets. Stable across versions
/// so external audits can grep for it.
pub const REDACTED: &str = "[REDACTED]";

/// Persistence port for the compliance bounded context.
#[async_trait]
pub trait ComplianceRepository: Send + Sync + 'static {
    /// Persist a hardening run.
    async fn save_hardening_run(&self, run: &HardeningRun) -> Result<(), RepoError>;
    /// Load a hardening run by id.
    async fn get_hardening_run(&self, id: Uuid) -> Result<Option<HardeningRun>, RepoError>;
    /// List recent hardening runs.
    async fn list_hardening_runs(&self, limit: u32) -> Result<Vec<HardeningRun>, RepoError>;

    /// Persist the retention policy (singleton).
    async fn save_retention_policy(&self, policy: &AuditRetentionPolicy) -> Result<(), RepoError>;
    /// Load the retention policy.
    async fn get_retention_policy(&self) -> Result<Option<AuditRetentionPolicy>, RepoError>;

    /// Persist a GDPR export.
    async fn save_gdpr_export(&self, export: &GdprExport) -> Result<(), RepoError>;
    /// Load a GDPR export by id.
    async fn get_gdpr_export(&self, id: Uuid) -> Result<Option<GdprExport>, RepoError>;
    /// List exports for a user, newest first.
    async fn list_gdpr_exports(&self, user_id: Uuid) -> Result<Vec<GdprExport>, RepoError>;
}

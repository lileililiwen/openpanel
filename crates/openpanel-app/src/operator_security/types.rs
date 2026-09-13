//! Operator security control-plane types: service errors, typed
//! remediation adapters, and remediation previews.

use openpanel_core::AuditAction;
use openpanel_domain::operator_security::{ControlPlaneError, FindingSource};
use thiserror::Error;
use uuid::Uuid;

/// Application failures with safe public messages.
#[derive(Debug, Error)]
pub enum ControlPlaneServiceError {
    /// Caller lacks permission.
    #[error("forbidden")]
    Forbidden,
    /// Finding does not exist.
    #[error("security finding not found")]
    NotFound,
    /// Domain validation rejected input.
    #[error("control-plane validation failed: {0}")]
    Validation(String),
    /// Remediation needs explicit confirmation first.
    #[error("remediation requires confirmation")]
    ConfirmationRequired,
    /// Finding has no automatic adapter; act manually.
    #[error("no automatic remediation; act manually: {0}")]
    ManualRequired(String),
    /// Adapter or persistence failure.
    #[error("control-plane unavailable")]
    Internal,
}

impl From<ControlPlaneError> for ControlPlaneServiceError {
    fn from(error: ControlPlaneError) -> Self {
        Self::Validation(error.to_string())
    }
}

/// One typed remediation adapter. Each variant wraps exactly one
/// existing service mutation; irreversible actions have no variant
/// and stay manual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRemediationKind {
    /// Review/apply managed firewall candidate (`SecurityService`).
    FirewallReview,
    /// Quarantine / restore a malware hit (`MalwareScannerService`).
    MalwareQuarantine,
    /// Tighten a per-site WAF ruleset (`WafService::put`).
    WafTighten,
    /// Roll back a hardening run (`HardeningRun::rollback_targets`).
    ComplianceRollback,
    /// Restart a degraded allowlisted service (`ServiceManager`).
    ServiceRestart,
}

impl ControlRemediationKind {
    /// Adapter for a finding source; `None` means manual-only.
    pub fn for_source(source: FindingSource) -> Option<Self> {
        match source {
            FindingSource::Firewall => Some(Self::FirewallReview),
            FindingSource::Malware => Some(Self::MalwareQuarantine),
            FindingSource::Waf => Some(Self::WafTighten),
            FindingSource::Compliance => Some(Self::ComplianceRollback),
            FindingSource::ServiceHealth => Some(Self::ServiceRestart),
        }
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FirewallReview => "firewall_review",
            Self::MalwareQuarantine => "malware_quarantine",
            Self::WafTighten => "waf_tighten",
            Self::ComplianceRollback => "compliance_rollback",
            Self::ServiceRestart => "service_restart",
        }
    }

    /// Whether the adapter can roll back after a failed post-check.
    /// Only adapters whose wrapped service owns a rollback path
    /// (`SecurityService::rollback`, hardening pre-images) qualify.
    pub fn supports_rollback(self) -> bool {
        match self {
            Self::FirewallReview | Self::ComplianceRollback => true,
            Self::MalwareQuarantine | Self::WafTighten | Self::ServiceRestart => false,
        }
    }

    /// Safe recovery copy shown when the post-check fails.
    pub fn recovery_guidance(self) -> &'static str {
        match self {
            Self::FirewallReview => {
                "Review the firewall preview; last-known-good rules are restored automatically. Re-apply from /security after fixing the candidate."
            }
            Self::MalwareQuarantine => {
                "Quarantine is kept; restore within 24h from the malware scanner page if the detection was a false positive."
            }
            Self::WafTighten => {
                "WAF ruleset was not persisted when the post-check failed. Re-run dry-run from the site WAF page."
            }
            Self::ComplianceRollback => {
                "Re-apply the hardening pre-image from the compliance run report; rollback targets are listed per rule."
            }
            Self::ServiceRestart => {
                "Service restart was refused or unhealthy; inspect the service page and logs before retrying."
            }
        }
    }

    /// Audit action reused from the wrapped bounded context.
    pub fn audit_action(self) -> AuditAction {
        match self {
            Self::FirewallReview => AuditAction::FirewallChanged,
            Self::MalwareQuarantine => AuditAction::QuarantineRecordCreated,
            Self::WafTighten => AuditAction::WafChanged,
            Self::ComplianceRollback => AuditAction::HardeningReverted,
            Self::ServiceRestart => AuditAction::ServiceChanged,
        }
    }
}

/// Preview shown before any automatic remediation executes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlRemediationPreview {
    finding_id: Uuid,
    kind: ControlRemediationKind,
    summary: String,
    steps: Vec<String>,
    requires_confirmation: bool,
    supports_rollback: bool,
}

impl ControlRemediationPreview {
    /// Build a preview for one finding and adapter.
    pub fn new(
        finding_id: Uuid,
        kind: ControlRemediationKind,
        summary: String,
        steps: Vec<String>,
        requires_confirmation: bool,
        supports_rollback: bool,
    ) -> Self {
        Self {
            finding_id,
            kind,
            summary,
            steps,
            requires_confirmation,
            supports_rollback,
        }
    }

    /// Finding under remediation.
    pub fn finding_id(&self) -> Uuid {
        self.finding_id
    }

    /// Typed adapter.
    pub fn kind(&self) -> ControlRemediationKind {
        self.kind
    }

    /// Human summary.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Ordered operator-visible steps.
    pub fn steps(&self) -> &[String] {
        &self.steps
    }

    /// Whether `confirmed=true` is required to execute.
    pub fn requires_confirmation(&self) -> bool {
        self.requires_confirmation
    }

    /// Whether rollback is supported.
    pub fn supports_rollback(&self) -> bool {
        self.supports_rollback
    }
}

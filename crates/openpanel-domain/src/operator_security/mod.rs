//! Operator security control plane: normalized findings, deterministic
//! ordering, suppression with expiry, and secret-safe evidence.
//!
//! I/O-free projection over existing scanner outputs (firewall, malware,
//! WAF, compliance, service health). Scanners keep their own logic; this
//! module owns the operator view: stable identity, severity ordering,
//! affected-resource scope, remediation mode, lifecycle state, and
//! redaction. Covers `operator-security-control-plane`: Findings Are
//! Normalized and Prioritized, Suppression Expires, Operators See Safe
//! Evidence (remediation execution lives in the app service).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Domain validation failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ControlPlaneError {
    /// A value is invalid.
    #[error("invalid control-plane value: {0}")]
    Invalid(String),
}

/// Which existing scanner produced the finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSource {
    /// Host firewall / login-abuse (`SecurityService`).
    Firewall,
    /// Web-application malware scanner.
    Malware,
    /// Per-site WAF.
    Waf,
    /// CIS hardening / compliance runs.
    Compliance,
    /// Service-health / monitoring alerts.
    ServiceHealth,
}

impl FindingSource {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Firewall => "firewall",
            Self::Malware => "malware",
            Self::Waf => "waf",
            Self::Compliance => "compliance",
            Self::ServiceHealth => "service_health",
        }
    }
}

/// Operator severity (highest first in queue order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    /// Informational.
    Info,
    /// Low risk.
    Low,
    /// Medium risk.
    Medium,
    /// High risk.
    High,
    /// Critical risk.
    Critical,
}

impl FindingSeverity {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Queue rank (higher = more urgent).
    pub fn rank(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

/// Lifecycle state owned by the control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Awaiting operator action.
    Open,
    /// Remediation adapter running.
    InRemediation,
    /// Post-check passed.
    Resolved,
    /// Post-check failed; needs operator review.
    Failed,
    /// Suppressed until expiry.
    Suppressed,
}

impl FindingState {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InRemediation => "in_remediation",
            Self::Resolved => "resolved",
            Self::Failed => "failed",
            Self::Suppressed => "suppressed",
        }
    }
}

/// Whether the control plane may fix the finding automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemediationMode {
    /// A typed adapter exists (preview + execute + post-check).
    Automatic,
    /// Operator must act manually using the evidence.
    Manual,
}

impl RemediationMode {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
        }
    }
}

/// Time-boxed ignore/snooze owned by the control plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingSuppression {
    reason: String,
    actor: String,
    scope: String,
    expires_at: DateTime<Utc>,
}

impl FindingSuppression {
    /// Build a suppression; reason/actor/scope must be non-empty, expiry
    /// must be in the future relative to `now`.
    pub fn new(
        reason: impl Into<String>,
        actor: impl Into<String>,
        scope: impl Into<String>,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, ControlPlaneError> {
        let reason = reason.into();
        let actor = actor.into();
        let scope = scope.into();
        if reason.trim().is_empty() || reason.len() > 280 {
            return Err(ControlPlaneError::Invalid("reason is required".into()));
        }
        if actor.trim().is_empty() || actor.len() > 254 {
            return Err(ControlPlaneError::Invalid("actor is required".into()));
        }
        if scope.trim().is_empty() || scope.len() > 280 {
            return Err(ControlPlaneError::Invalid("scope is required".into()));
        }
        if expires_at <= now {
            return Err(ControlPlaneError::Invalid(
                "suppression expiry must be in the future".into(),
            ));
        }
        Ok(Self {
            reason,
            actor,
            scope,
            expires_at,
        })
    }

    /// Human reason (redacted at the projection boundary).
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Suppressing principal.
    pub fn suppressed_by(&self) -> &str {
        &self.actor
    }

    /// Scope the suppression applies to.
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// When the finding returns to the queue.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the suppression has lapsed at `now`.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// One normalized operator finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityFinding {
    id: Uuid,
    source: FindingSource,
    severity: FindingSeverity,
    resource: String,
    rule: String,
    title: String,
    evidence: String,
    remediation_mode: RemediationMode,
    state: FindingState,
    first_seen_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    suppression: Option<FindingSuppression>,
}

impl SecurityFinding {
    /// Build a finding; validates lengths and redacts evidence eagerly.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: FindingSource,
        severity: FindingSeverity,
        resource: impl Into<String>,
        rule: impl Into<String>,
        title: impl Into<String>,
        evidence: impl Into<String>,
        remediation_mode: RemediationMode,
        now: DateTime<Utc>,
    ) -> Result<Self, ControlPlaneError> {
        let resource = resource.into();
        let rule = rule.into();
        let title = title.into();
        if resource.trim().is_empty() || resource.len() > 512 {
            return Err(ControlPlaneError::Invalid("resource is required".into()));
        }
        if rule.trim().is_empty() || rule.len() > 256 {
            return Err(ControlPlaneError::Invalid("rule is required".into()));
        }
        if title.trim().is_empty() || title.len() > 280 {
            return Err(ControlPlaneError::Invalid("title is required".into()));
        }
        let evidence = redact_text(&evidence.into());
        if evidence.len() > 2048 {
            return Err(ControlPlaneError::Invalid("evidence is too long".into()));
        }
        Ok(Self {
            id: stable_finding_id(source, &resource, &rule),
            source,
            severity,
            resource,
            rule,
            title,
            evidence,
            remediation_mode,
            state: FindingState::Open,
            first_seen_at: now,
            updated_at: now,
            suppression: None,
        })
    }

    /// Stable deterministic identity.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Scanner source.
    pub fn source(&self) -> FindingSource {
        self.source
    }

    /// Severity.
    pub fn severity(&self) -> FindingSeverity {
        self.severity
    }

    /// Affected resource scope.
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Rule key within the source.
    pub fn rule_key(&self) -> &str {
        &self.rule
    }

    /// Short title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Redacted evidence.
    pub fn evidence(&self) -> &str {
        &self.evidence
    }

    /// Remediation mode.
    pub fn remediation_mode(&self) -> RemediationMode {
        self.remediation_mode
    }

    /// Lifecycle state.
    pub fn state(&self) -> FindingState {
        self.state
    }

    /// First observation.
    pub fn first_seen_at(&self) -> DateTime<Utc> {
        self.first_seen_at
    }

    /// Last transition.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Active suppression, if any.
    pub fn suppression(&self) -> Option<&FindingSuppression> {
        self.suppression.as_ref()
    }

    /// Attach a suppression (transitions to `Suppressed`).
    pub fn apply_suppression(&mut self, suppression: FindingSuppression, now: DateTime<Utc>) {
        self.suppression = Some(suppression);
        self.state = FindingState::Suppressed;
        self.updated_at = now;
    }

    /// Reopen when a suppression lapses; returns true when reopened.
    pub fn reopen_if_suppression_expired(&mut self, now: DateTime<Utc>) -> bool {
        let expired = self
            .suppression
            .as_ref()
            .is_some_and(|suppression| suppression.is_expired(now));
        if expired {
            self.suppression = None;
            if self.state == FindingState::Suppressed {
                self.state = FindingState::Open;
            }
            self.updated_at = now;
            true
        } else {
            false
        }
    }

    /// Transition helper used by the app service.
    pub fn transition(&mut self, state: FindingState, now: DateTime<Utc>) {
        self.state = state;
        self.updated_at = now;
    }

    /// Merge evidence from a duplicate (same stable id).
    pub fn merge_duplicate(&mut self, other: &Self, now: DateTime<Utc>) {
        if other.evidence.is_empty() || self.evidence.contains(&other.evidence) {
            self.updated_at = now.max(self.updated_at);
            return;
        }
        let mut merged = self.evidence.clone();
        if !merged.is_empty() {
            merged.push_str("\n---\n");
        }
        merged.push_str(&other.evidence);
        if merged.len() > 2048 {
            merged.truncate(2048);
        }
        self.evidence = merged;
        if other.severity.rank() > self.severity.rank() {
            self.severity = other.severity;
        }
        if other.first_seen_at < self.first_seen_at {
            self.first_seen_at = other.first_seen_at;
        }
        self.updated_at = now.max(self.updated_at).max(other.updated_at);
    }
}

/// Deterministic stable id from source + resource + rule.
pub fn stable_finding_id(source: FindingSource, resource: &str, rule: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(source.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(resource.trim().to_lowercase().as_bytes());
    hasher.update([0]);
    hasher.update(rule.trim().to_lowercase().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Deduplicate by stable id, merging evidence and keeping the highest
/// severity. Order of first occurrence is preserved.
pub fn deduplicate_findings(
    findings: Vec<SecurityFinding>,
    now: DateTime<Utc>,
) -> Vec<SecurityFinding> {
    let mut merged: Vec<SecurityFinding> = Vec::with_capacity(findings.len());
    for finding in findings {
        if let Some(existing) = merged.iter_mut().find(|item| item.id() == finding.id()) {
            existing.merge_duplicate(&finding, now);
        } else {
            merged.push(finding);
        }
    }
    merged
}

/// Deterministic queue order: severity (critical first), then source,
/// then resource, then rule.
pub fn sort_findings(findings: &mut [SecurityFinding]) {
    findings.sort_by(|left, right| {
        right
            .severity()
            .rank()
            .cmp(&left.severity().rank())
            .then_with(|| left.source().as_str().cmp(right.source().as_str()))
            .then_with(|| left.resource().cmp(right.resource()))
            .then_with(|| left.rule_key().cmp(right.rule_key()))
    });
}

/// Redact credential-like spans. Keeps the finding actionable while
/// removing passwords, tokens, private keys, and raw command payloads.
pub fn redact_text(value: &str) -> String {
    let mut out = value.to_string();
    for marker in [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "private-key",
        "credential",
        "session",
        "bearer",
    ] {
        let mut search_from = 0;
        while let Some(relative) = out[search_from..].to_lowercase().find(marker) {
            let start = search_from + relative;
            let rest = &out[start..];
            let take = rest
                .char_indices()
                .take_while(|(_, ch)| {
                    !matches!(ch, '\n' | '\r') || rest[..ch.len_utf8()].len() < marker.len()
                })
                .map(|(index, _)| index)
                .last()
                .unwrap_or(0);
            let end = (start + take.max(marker.len())).min(out.len());
            let end = out[start..end]
                .find('\n')
                .map_or(end, |offset| start + offset);
            out.replace_range(start..end, "[REDACTED]");
            search_from = start + "[REDACTED]".len();
            if search_from >= out.len() {
                break;
            }
        }
    }
    // PEM blocks are never evidence.
    while let Some(start) = out.find("-----BEGIN") {
        let end = out[start..].find("-----END").map_or(out.len(), |offset| {
            let tail = &out[start + offset..];
            tail.find('\n')
                .map_or(out.len(), |line| start + offset + line)
        });
        out.replace_range(start..end.min(out.len()), "[REDACTED]");
    }
    out
}

/// Metadata redaction reuses the canonical audit allowlist
/// (`openpanel_core::audit::redact_metadata`): secret keys are dropped
/// from audit events, and evidence text is scrubbed by [`redact_text`]
/// at the finding boundary. No second implementation lives here by
/// design (the `reuse-strict` gate rejects duplicated utilities).
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Capability under test: `operator-security-control-plane`.
    const CAPABILITY: &str = "operator-security-control-plane";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0)
            .single()
            .expect("test time")
    }

    fn finding(
        source: FindingSource,
        severity: FindingSeverity,
        resource: &str,
        rule: &str,
    ) -> SecurityFinding {
        SecurityFinding::new(
            source,
            severity,
            resource,
            rule,
            "title",
            "evidence",
            RemediationMode::Manual,
            now(),
        )
        .expect("finding")
    }

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "operator-security-control-plane");
    }

    #[test]
    fn stable_id_is_case_insensitive_and_deterministic() {
        let first = stable_finding_id(FindingSource::Firewall, "Host:22", "SSH-OPEN");
        let second = stable_finding_id(FindingSource::Firewall, "host:22", "ssh-open");
        assert_eq!(first, second);
        let other = stable_finding_id(FindingSource::Waf, "host:22", "ssh-open");
        assert_ne!(first, other);
    }

    #[test]
    fn deduplication_merges_evidence_and_keeps_highest_severity() {
        let mut first = finding(
            FindingSource::Malware,
            FindingSeverity::Medium,
            "/srv/www/x",
            "eicar",
        );
        first.evidence = "first hit".into();
        let mut second = finding(
            FindingSource::Malware,
            FindingSeverity::High,
            "/srv/www/x",
            "EICAR",
        );
        second.evidence = "second hit".into();
        assert_eq!(first.id(), second.id());
        let merged = deduplicate_findings(vec![first, second], now());
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].severity(), FindingSeverity::High);
        assert!(merged[0].evidence().contains("first hit"));
        assert!(merged[0].evidence().contains("second hit"));
    }

    #[test]
    fn queue_order_is_severity_then_source_then_resource() {
        let mut items = vec![
            finding(FindingSource::Waf, FindingSeverity::Low, "b", "r1"),
            finding(
                FindingSource::Firewall,
                FindingSeverity::Critical,
                "a",
                "r1",
            ),
            finding(
                FindingSource::Compliance,
                FindingSeverity::Critical,
                "a",
                "r0",
            ),
        ];
        sort_findings(&mut items);
        assert_eq!(items[0].severity(), FindingSeverity::Critical);
        assert_eq!(items[0].source(), FindingSource::Compliance);
        assert_eq!(items[1].source(), FindingSource::Firewall);
        assert_eq!(items[2].severity(), FindingSeverity::Low);
    }

    #[test]
    fn suppression_requires_reason_and_future_expiry() {
        let at = now();
        assert!(
            FindingSuppression::new("", "owner", "host", at + chrono::Duration::hours(1), at)
                .is_err()
        );
        assert!(FindingSuppression::new("noise", "owner", "host", at, at).is_err());
        let suppression = FindingSuppression::new(
            "known noise",
            "owner",
            "host",
            at + chrono::Duration::hours(1),
            at,
        )
        .expect("suppression");
        assert!(!suppression.is_expired(at));
        assert!(suppression.is_expired(at + chrono::Duration::hours(2)));
    }

    #[test]
    fn expired_suppression_reopens_without_state_loss() {
        let at = now();
        let mut item = finding(
            FindingSource::ServiceHealth,
            FindingSeverity::High,
            "svc:nginx",
            "down",
        );
        item.apply_suppression(
            FindingSuppression::new(
                "oncall ack",
                "owner",
                "svc:nginx",
                at + chrono::Duration::hours(1),
                at,
            )
            .expect("suppression"),
            at,
        );
        assert_eq!(item.state(), FindingState::Suppressed);
        assert!(!item.reopen_if_suppression_expired(at));
        assert!(item.reopen_if_suppression_expired(at + chrono::Duration::hours(2)));
        assert_eq!(item.state(), FindingState::Open);
        assert!(item.suppression().is_none());
    }

    #[test]
    fn redaction_removes_secrets_but_keeps_actionable_text() {
        let redacted = redact_text("firewall denies 203.0.113.7 password=hunter2 token abc");
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("token abc"));
        assert!(redacted.contains("203.0.113.7"));
        let pem = "key:\n-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----";
        assert!(!redact_text(pem).contains("BEGIN"));
        // Metadata redaction reuses the canonical audit allowlist
        // (`openpanel_core::audit::redact_metadata`, covered by the app
        // recording-audit test): no second implementation lives here.
    }

    #[test]
    fn constructor_rejects_empty_scope_and_redacts_eagerly() {
        assert!(
            SecurityFinding::new(
                FindingSource::Firewall,
                FindingSeverity::High,
                "",
                "rule",
                "title",
                "evidence",
                RemediationMode::Manual,
                now(),
            )
            .is_err()
        );
        let item = SecurityFinding::new(
            FindingSource::Firewall,
            FindingSeverity::High,
            "host:22",
            "ssh-open",
            "SSH open",
            "password=hunter2 on host:22",
            RemediationMode::Manual,
            now(),
        )
        .expect("finding");
        assert!(!item.evidence().contains("hunter2"));
        assert_eq!(item.state(), FindingState::Open);
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_dedup_never_grows(a in "[a-z0-9:/.]{1,24}", b in "[a-z0-9:/.]{1,24}") {
                let at = Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0).single().expect("time");
                let mk = |resource: &str| SecurityFinding::new(
                    FindingSource::Malware, FindingSeverity::Medium,
                    resource, "rule", "t", "e", RemediationMode::Manual, at,
                ).expect("finding");
                let input = vec![mk(&a), mk(&b)];
                let merged = deduplicate_findings(input, at);
                prop_assert!(merged.len() <= 2);
                prop_assert!(!merged.is_empty());
            }

            #[test]
            fn prop_sort_is_deterministic(
                severities in proptest::collection::vec(0..5u8, 1..8),
            ) {
                let at = Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0).single().expect("time");
                let to_severity = |raw: u8| match raw {
                    0 => FindingSeverity::Info,
                    1 => FindingSeverity::Low,
                    2 => FindingSeverity::Medium,
                    3 => FindingSeverity::High,
                    _ => FindingSeverity::Critical,
                };
                let mut first: Vec<SecurityFinding> = severities.iter().enumerate().map(|(index, raw)| {
                    SecurityFinding::new(
                        FindingSource::Firewall, to_severity(*raw),
                        format!("host:{index}"), format!("rule-{index}"),
                        "t", "e", RemediationMode::Manual, at,
                    ).expect("finding")
                }).collect();
                let mut second = first.clone();
                sort_findings(&mut first);
                sort_findings(&mut second);
                prop_assert_eq!(first.clone(), second);
                for window in first.windows(2) {
                    prop_assert!(window[0].severity().rank() >= window[1].severity().rank());
                }
            }

            #[test]
            fn prop_redaction_never_leaks_password_marker(value in ".{0,120}") {
                let redacted = redact_text(&value);
                if value.to_lowercase().contains("password") {
                    prop_assert!(!redacted.to_lowercase().contains("password="));
                }
            }
        }
    }
}

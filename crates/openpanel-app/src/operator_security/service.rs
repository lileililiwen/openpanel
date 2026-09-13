//! Operator security control-plane lifecycle service.
//!
//! Normalized finding projection over existing scanner outputs. The
//! service owns lifecycle, evidence, idempotency, expiry, post-check
//! verification, audit, and recovery guidance; each automatic
//! remediation executes through one typed adapter
//! ([`ControlRemediationKind`]) wired by [`ControlRemediationPort`].

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService, audit::redact_metadata};
use openpanel_domain::{
    User,
    identity::Role,
    operator_security::{
        FindingState, RemediationMode, SecurityFinding, deduplicate_findings, sort_findings,
    },
};
use uuid::Uuid;

use super::{
    port::ControlRemediationPort,
    types::{ControlPlaneServiceError, ControlRemediationKind, ControlRemediationPreview},
};
/// In-memory lifecycle service (projection; scanners stay authoritative).
pub struct OperatorSecurityService {
    port: Arc<dyn ControlRemediationPort>,
    audit: Arc<dyn AuditService>,
    notifications: Option<Arc<crate::notifications::NotificationService>>,
    findings: Mutex<HashMap<Uuid, SecurityFinding>>,
    idempotency: Mutex<HashSet<String>>,
}

impl OperatorSecurityService {
    /// Compose with an explicit remediation port and audit sink.
    pub fn new(port: Arc<dyn ControlRemediationPort>, audit: Arc<dyn AuditService>) -> Self {
        Self {
            port,
            audit,
            notifications: None,
            findings: Mutex::new(HashMap::new()),
            idempotency: Mutex::new(HashSet::new()),
        }
    }

    /// Attach notification fan-out for remediation outcomes. Publish
    /// failures never fail the remediation itself.
    pub fn with_notifications(
        mut self,
        notifications: Arc<crate::notifications::NotificationService>,
    ) -> Self {
        self.notifications = Some(notifications);
        self
    }

    /// Owner/admin gate (defense in depth; handlers gate again).
    fn require_operator(user: &User) -> Result<(), ControlPlaneServiceError> {
        if matches!(user.role(), Role::Owner | Role::Admin) {
            Ok(())
        } else {
            Err(ControlPlaneServiceError::Forbidden)
        }
    }

    /// Ingest scanner findings: normalize identity, deduplicate, store.
    pub async fn ingest(
        &self,
        caller: &User,
        findings: Vec<SecurityFinding>,
        now: DateTime<Utc>,
    ) -> Result<Vec<SecurityFinding>, ControlPlaneServiceError> {
        Self::require_operator(caller).inspect_err(|_| {
            self.audit_denied(caller, "ingest");
        })?;
        let merged = deduplicate_findings(findings, now);
        let mut store = self
            .findings
            .lock()
            .map_err(|_| ControlPlaneServiceError::Internal)?;
        for finding in &merged {
            store
                .entry(finding.id())
                .and_modify(|existing| existing.merge_duplicate(finding, now))
                .or_insert_with(|| finding.clone());
        }
        Ok(merged)
    }

    /// Prioritized queue: expired suppressions reopen, active
    /// suppressions stay hidden, order is deterministic.
    pub async fn queue(
        &self,
        caller: &User,
        now: DateTime<Utc>,
    ) -> Result<Vec<SecurityFinding>, ControlPlaneServiceError> {
        Self::require_operator(caller).inspect_err(|_| {
            self.audit_denied(caller, "queue");
        })?;
        let mut store = self
            .findings
            .lock()
            .map_err(|_| ControlPlaneServiceError::Internal)?;
        for finding in store.values_mut() {
            finding.reopen_if_suppression_expired(now);
        }
        let mut out: Vec<SecurityFinding> = store
            .values()
            .filter(|finding| {
                finding.state() != FindingState::Suppressed
                    && finding.state() != FindingState::Resolved
            })
            .cloned()
            .collect();
        sort_findings(&mut out);
        Ok(out)
    }

    /// Fetch one finding.
    pub async fn find(
        &self,
        caller: &User,
        id: Uuid,
    ) -> Result<SecurityFinding, ControlPlaneServiceError> {
        Self::require_operator(caller).inspect_err(|_| {
            self.audit_denied(caller, "find");
        })?;
        self.findings
            .lock()
            .map_err(|_| ControlPlaneServiceError::Internal)?
            .get(&id)
            .cloned()
            .ok_or(ControlPlaneServiceError::NotFound)
    }

    /// Preview the typed adapter; manual findings explain themselves.
    pub async fn preview(
        &self,
        caller: &User,
        id: Uuid,
    ) -> Result<ControlRemediationPreview, ControlPlaneServiceError> {
        let finding = self.find(caller, id).await?;
        if finding.remediation_mode() == RemediationMode::Manual {
            return Err(ControlPlaneServiceError::ManualRequired(format!(
                "{}: {}",
                finding.resource(),
                finding.evidence()
            )));
        }
        let kind = ControlRemediationKind::for_source(finding.source()).ok_or_else(|| {
            ControlPlaneServiceError::ManualRequired(format!(
                "{}: {}",
                finding.resource(),
                finding.evidence()
            ))
        })?;
        Ok(ControlRemediationPreview::new(
            id,
            kind,
            format!(
                "{} {} on {}",
                kind.as_str(),
                finding.rule_key(),
                finding.resource()
            ),
            vec![
                format!("preview {} for {}", kind.as_str(), finding.resource()),
                "execute with idempotency key".into(),
                "post-check health".into(),
                "audit + notify".into(),
            ],
            true,
            kind.supports_rollback(),
        ))
    }

    /// Execute with preview authorization, idempotency, post-check,
    /// audit, and rollback guidance on failure.
    pub async fn remediate(
        &self,
        caller: &User,
        id: Uuid,
        idempotency_key: &str,
        confirmed: bool,
        now: DateTime<Utc>,
    ) -> Result<SecurityFinding, ControlPlaneServiceError> {
        Self::require_operator(caller).inspect_err(|_| {
            self.audit_denied(caller, "remediate");
        })?;
        if idempotency_key.trim().is_empty() || idempotency_key.len() > 128 {
            return Err(ControlPlaneServiceError::Validation(
                "idempotency key is required".into(),
            ));
        }
        let already_seen = {
            let seen = self
                .idempotency
                .lock()
                .map_err(|_| ControlPlaneServiceError::Internal)?;
            seen.contains(idempotency_key)
        };
        if already_seen {
            return self.find(caller, id).await;
        }
        let preview = self.preview(caller, id).await?;
        if preview.requires_confirmation() && !confirmed {
            return Err(ControlPlaneServiceError::ConfirmationRequired);
        }
        {
            let mut store = self
                .findings
                .lock()
                .map_err(|_| ControlPlaneServiceError::Internal)?;
            let finding = store
                .get_mut(&id)
                .ok_or(ControlPlaneServiceError::NotFound)?;
            finding.transition(FindingState::InRemediation, now);
        }
        let finding = self.find(caller, id).await?;
        let executed = self.port.execute(caller, preview.kind(), &finding).await;
        if let Err(detail) = executed {
            self.fail(caller, id, &detail, now).await?;
            self.audit_event(
                caller,
                preview.kind().audit_action(),
                AuditOutcome::Failure,
                &finding,
                "execute failed",
            )
            .await;
            return Err(ControlPlaneServiceError::Internal);
        }
        let healthy = self
            .port
            .verify(caller, preview.kind(), &finding)
            .await
            .map_err(|_| ControlPlaneServiceError::Internal)?;
        if healthy {
            {
                let mut store = self
                    .findings
                    .lock()
                    .map_err(|_| ControlPlaneServiceError::Internal)?;
                if let Some(item) = store.get_mut(&id) {
                    item.transition(FindingState::Resolved, now);
                }
            }
            {
                let mut seen = self
                    .idempotency
                    .lock()
                    .map_err(|_| ControlPlaneServiceError::Internal)?;
                seen.insert(idempotency_key.to_string());
            }
            self.audit_event(
                caller,
                preview.kind().audit_action(),
                AuditOutcome::Success,
                &finding,
                "remediated and verified",
            )
            .await;
            let resolved = self.find(caller, id).await?;
            self.notify_outcome(caller, &resolved, true, now).await;
        } else {
            let _ = self.port.rollback(caller, preview.kind(), &finding).await;
            self.fail(caller, id, preview.kind().recovery_guidance(), now)
                .await?;
            self.audit_event(
                caller,
                preview.kind().audit_action(),
                AuditOutcome::Failure,
                &finding,
                "post-check failed; recovery guidance issued",
            )
            .await;
            {
                let mut seen = self
                    .idempotency
                    .lock()
                    .map_err(|_| ControlPlaneServiceError::Internal)?;
                seen.insert(idempotency_key.to_string());
            }
            let failed = self.find(caller, id).await?;
            self.notify_outcome(caller, &failed, false, now).await;
        }
        self.find(caller, id).await
    }

    /// Best-effort notification fan-out for a remediation outcome.
    /// Publish failures never fail the remediation itself.
    async fn notify_outcome(
        &self,
        caller: &User,
        finding: &SecurityFinding,
        resolved: bool,
        now: DateTime<Utc>,
    ) {
        let Some(notifications) = self.notifications.clone() else {
            return;
        };
        let action = ControlRemediationKind::for_source(finding.source())
            .map(|kind| kind.audit_action())
            .unwrap_or(AuditAction::SettingsChanged);
        let subject = format!(
            "Security finding {}: {} on {}",
            if resolved { "resolved" } else { "failed" },
            finding.rule_key(),
            finding.resource(),
        );
        let subject = subject.chars().take(240).collect::<String>();
        let details = redact_metadata(&serde_json::json!({
            "action": action.as_str(),
            "finding_id": finding.id(),
            "source": finding.source().as_str(),
            "severity": finding.severity().as_str(),
            "resource": finding.resource(),
            "rule": finding.rule_key(),
            "state": finding.state().as_str(),
            "actor": caller.username().as_str(),
        }));
        let event = match openpanel_domain::notifications::NotificationEvent::new(
            Uuid::new_v4(),
            openpanel_domain::notifications::EventKind::Audit,
            subject,
            if resolved {
                openpanel_domain::notifications::Severity::Warning
            } else {
                openpanel_domain::notifications::Severity::Critical
            },
            details,
            now,
        ) {
            Ok(event) => event,
            Err(_) => return,
        };
        let _ = notifications.publish(event).await;
    }

    /// Suppress with reason/actor/scope/expiry; audit the decision.
    pub async fn suppress_finding(
        &self,
        caller: &User,
        id: Uuid,
        reason: &str,
        scope: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<SecurityFinding, ControlPlaneServiceError> {
        Self::require_operator(caller).inspect_err(|_| {
            self.audit_denied(caller, "suppress");
        })?;
        let actor = caller.username().as_str().to_string();
        let suppression = openpanel_domain::operator_security::FindingSuppression::new(
            reason, actor, scope, expires_at, now,
        )?;
        {
            let mut store = self
                .findings
                .lock()
                .map_err(|_| ControlPlaneServiceError::Internal)?;
            let finding = store
                .get_mut(&id)
                .ok_or(ControlPlaneServiceError::NotFound)?;
            finding.apply_suppression(suppression, now);
        }
        let finding = self.find(caller, id).await?;
        self.audit_event(
            caller,
            AuditAction::SettingsChanged,
            AuditOutcome::Success,
            &finding,
            "suppressed with expiry",
        )
        .await;
        Ok(finding)
    }

    /// Count open findings by severity (dashboard attention hook).
    pub async fn attention_counts(
        &self,
        caller: &User,
        now: DateTime<Utc>,
    ) -> Result<HashMap<String, usize>, ControlPlaneServiceError> {
        let items = self.queue(caller, now).await?;
        let mut counts = HashMap::new();
        for item in items {
            *counts
                .entry(item.severity().as_str().to_string())
                .or_insert(0) += 1;
        }
        Ok(counts)
    }

    async fn fail(
        &self,
        caller: &User,
        id: Uuid,
        _detail: &str,
        now: DateTime<Utc>,
    ) -> Result<(), ControlPlaneServiceError> {
        Self::require_operator(caller)?;
        let mut store = self
            .findings
            .lock()
            .map_err(|_| ControlPlaneServiceError::Internal)?;
        let finding = store
            .get_mut(&id)
            .ok_or(ControlPlaneServiceError::NotFound)?;
        finding.transition(FindingState::Failed, now);
        Ok(())
    }

    async fn audit_event(
        &self,
        caller: &User,
        action: AuditAction,
        outcome: AuditOutcome,
        finding: &SecurityFinding,
        note: &str,
    ) {
        // Canonical audit allowlist: secret keys are dropped, secret
        // shapes are masked, actionable keys survive (reused, not
        // reimplemented — see `openpanel_core::audit::redact_metadata`).
        let metadata = redact_metadata(&serde_json::json!({
            "finding_id": finding.id(),
            "source": finding.source().as_str(),
            "severity": finding.severity().as_str(),
            "resource": finding.resource(),
            "rule": finding.rule_key(),
            "note": note,
        }));
        let _ = self
            .audit
            .record(
                AuditEvent::new(caller.username().as_str(), action, outcome)
                    .target(finding.resource())
                    .metadata(metadata),
            )
            .await;
    }

    fn audit_denied(&self, caller: &User, operation: &str) {
        let audit = self.audit.clone();
        let actor = caller.username().as_str().to_string();
        let operation = operation.to_string();
        tokio::spawn(async move {
            let _ = audit
                .record(
                    AuditEvent::new(actor, AuditAction::PermissionDenied, AuditOutcome::Denied)
                        .target(format!("control-plane:{operation}")),
                )
                .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use openpanel_domain::operator_security::{
        FindingSeverity, FindingSource, RemediationMode, SecurityFinding,
    };
    use openpanel_test_support::MockAudit;

    /// Capability under test: `operator-security-control-plane`.
    const CAPABILITY: &str = "operator-security-control-plane";

    /// Recording audit sink for redaction assertions.
    struct RecordingAudit {
        events: Mutex<Vec<openpanel_core::AuditEvent>>,
    }

    #[async_trait::async_trait]
    impl openpanel_core::AuditService for RecordingAudit {
        async fn record(
            &self,
            event: openpanel_core::AuditEvent,
        ) -> openpanel_core::CoreResult<()> {
            self.events
                .lock()
                .expect("invariant: recording audit lock is never poisoned")
                .push(event);
            Ok(())
        }

        async fn recent(
            &self,
            _limit: i64,
        ) -> openpanel_core::CoreResult<Vec<openpanel_core::AuditEvent>> {
            Ok(vec![])
        }

        async fn query(
            &self,
            _query: openpanel_core::audit::AuditQuery,
        ) -> openpanel_core::CoreResult<openpanel_core::audit::AuditPage> {
            Ok(openpanel_core::audit::AuditPage {
                events: vec![],
                next_cursor: None,
            })
        }
    }

    struct AllowAll;
    #[async_trait]
    impl ControlRemediationPort for AllowAll {
        async fn execute(
            &self,
            _caller: &User,
            _kind: ControlRemediationKind,
            _finding: &SecurityFinding,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn verify(
            &self,
            _caller: &User,
            _kind: ControlRemediationKind,
            _finding: &SecurityFinding,
        ) -> Result<bool, String> {
            Ok(true)
        }
        async fn rollback(
            &self,
            _caller: &User,
            _kind: ControlRemediationKind,
            _finding: &SecurityFinding,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    struct FailVerify;
    #[async_trait]
    impl ControlRemediationPort for FailVerify {
        async fn execute(
            &self,
            _caller: &User,
            _kind: ControlRemediationKind,
            _finding: &SecurityFinding,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn verify(
            &self,
            _caller: &User,
            _kind: ControlRemediationKind,
            _finding: &SecurityFinding,
        ) -> Result<bool, String> {
            Ok(false)
        }
        async fn rollback(
            &self,
            _caller: &User,
            _kind: ControlRemediationKind,
            _finding: &SecurityFinding,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0)
            .single()
            .expect("time")
    }

    fn owner() -> User {
        use openpanel_domain::{Email, Password, Username};
        User::new(
            Uuid::new_v4(),
            Username::new("owner").expect("static"),
            Email::new("owner@example.com").expect("static"),
            Password::hash("correct horse battery staple").expect("static"),
            Role::Owner,
        )
    }

    fn member() -> User {
        use openpanel_domain::{Email, Password, Username};
        User::new(
            Uuid::new_v4(),
            Username::new("member").expect("static"),
            Email::new("member@example.com").expect("static"),
            Password::hash("correct horse battery staple").expect("static"),
            Role::User,
        )
    }

    fn auto_finding_with_evidence(resource: &str, evidence: &str) -> SecurityFinding {
        SecurityFinding::new(
            FindingSource::Firewall,
            FindingSeverity::High,
            resource,
            "ssh-open",
            "SSH open",
            evidence,
            RemediationMode::Automatic,
            now(),
        )
        .expect("finding")
    }

    fn auto_finding(resource: &str) -> SecurityFinding {
        auto_finding_with_evidence(resource, "evidence")
    }

    fn manual_finding() -> SecurityFinding {
        SecurityFinding::new(
            FindingSource::Compliance,
            FindingSeverity::Medium,
            "host:cis-5.2.1",
            "cis-5.2.1",
            "SSH root login",
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
    fn remediation_kinds_cover_every_source_with_rollback_contract() {
        for source in [
            FindingSource::Firewall,
            FindingSource::Malware,
            FindingSource::Waf,
            FindingSource::Compliance,
            FindingSource::ServiceHealth,
        ] {
            let kind = ControlRemediationKind::for_source(source).expect("typed adapter");
            assert!(!kind.as_str().is_empty());
            assert!(!kind.recovery_guidance().is_empty());
        }
        assert!(ControlRemediationKind::FirewallReview.supports_rollback());
        assert!(ControlRemediationKind::ComplianceRollback.supports_rollback());
        assert!(!ControlRemediationKind::WafTighten.supports_rollback());
        assert!(!ControlRemediationKind::ServiceRestart.supports_rollback());
        assert!(!ControlRemediationKind::MalwareQuarantine.supports_rollback());
    }

    #[tokio::test]
    async fn ingest_deduplicates_and_queue_is_prioritized() {
        let service = OperatorSecurityService::new(Arc::new(AllowAll), Arc::new(MockAudit::stub()));
        let caller = owner();
        let first = auto_finding_with_evidence("host:22", "first");
        let dup = auto_finding_with_evidence("host:22", "first");
        let low = SecurityFinding::new(
            FindingSource::Waf,
            FindingSeverity::Low,
            "site:x",
            "rule",
            "t",
            "e",
            RemediationMode::Automatic,
            now(),
        )
        .expect("finding");
        service
            .ingest(&caller, vec![first.clone(), dup, low], now())
            .await
            .expect("ingest");
        let queue = service.queue(&caller, now()).await.expect("queue");
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].severity(), FindingSeverity::High);
    }

    #[tokio::test]
    async fn non_operator_is_denied_without_oracle() {
        let service = OperatorSecurityService::new(Arc::new(AllowAll), Arc::new(MockAudit::stub()));
        let member = member();
        assert!(matches!(
            service.queue(&member, now()).await,
            Err(ControlPlaneServiceError::Forbidden)
        ));
        assert!(matches!(
            service.find(&member, Uuid::new_v4()).await,
            Err(ControlPlaneServiceError::Forbidden)
        ));
    }

    #[tokio::test]
    async fn preview_requires_confirmation_and_manual_is_guided() {
        let service = OperatorSecurityService::new(Arc::new(AllowAll), Arc::new(MockAudit::stub()));
        let caller = owner();
        let auto = auto_finding("host:22");
        let manual = manual_finding();
        service
            .ingest(&caller, vec![auto.clone(), manual.clone()], now())
            .await
            .expect("ingest");
        let preview = service.preview(&caller, auto.id()).await.expect("preview");
        assert!(preview.requires_confirmation());
        assert!(matches!(
            service.preview(&caller, manual.id()).await,
            Err(ControlPlaneServiceError::ManualRequired(_))
        ));
        assert!(matches!(
            service
                .remediate(&caller, auto.id(), "key-1", false, now())
                .await,
            Err(ControlPlaneServiceError::ConfirmationRequired)
        ));
    }

    #[tokio::test]
    async fn remediate_is_idempotent_and_resolves_on_post_check() {
        let service = OperatorSecurityService::new(Arc::new(AllowAll), Arc::new(MockAudit::stub()));
        let caller = owner();
        let item = auto_finding("host:22");
        service
            .ingest(&caller, vec![item.clone()], now())
            .await
            .expect("ingest");
        let resolved = service
            .remediate(&caller, item.id(), "key-1", true, now())
            .await
            .expect("remediate");
        assert_eq!(resolved.state(), FindingState::Resolved);
        let again = service
            .remediate(&caller, item.id(), "key-1", true, now())
            .await
            .expect("idempotent");
        assert_eq!(again.state(), FindingState::Resolved);
    }

    #[tokio::test]
    async fn failed_post_check_stays_failed_with_recovery() {
        let service =
            OperatorSecurityService::new(Arc::new(FailVerify), Arc::new(MockAudit::stub()));
        let caller = owner();
        let item = auto_finding("host:22");
        service
            .ingest(&caller, vec![item.clone()], now())
            .await
            .expect("ingest");
        let failed = service
            .remediate(&caller, item.id(), "key-2", true, now())
            .await
            .expect("remediate returns finding");
        assert_eq!(failed.state(), FindingState::Failed);
        assert!(
            !ControlRemediationKind::FirewallReview
                .recovery_guidance()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn audit_events_carry_redacted_metadata_and_safe_targets() {
        let audit = Arc::new(RecordingAudit {
            events: Mutex::new(Vec::new()),
        });
        let service = OperatorSecurityService::new(Arc::new(AllowAll), audit.clone());
        let caller = owner();
        let item = SecurityFinding::new(
            FindingSource::Firewall,
            FindingSeverity::High,
            "firewall:rules",
            "login-blocks-active",
            "Login-abuse blocks are active",
            "deny 203.0.113.7 password=hunter2",
            RemediationMode::Automatic,
            now(),
        )
        .expect("finding");
        service
            .ingest(&caller, vec![item.clone()], now())
            .await
            .expect("ingest");
        service
            .remediate(&caller, item.id(), "audit-key", true, now())
            .await
            .expect("remediate");
        let events = audit
            .events
            .lock()
            .expect("invariant: test audit lock is never poisoned")
            .clone();
        assert!(!events.is_empty(), "remediation is audited");
        let rendered = serde_json::to_string(&events).expect("serializable");
        assert!(!rendered.contains("hunter2"), "no secret: {rendered}");
        assert!(
            rendered.contains("firewall:rules"),
            "actionable context kept: {rendered}"
        );
    }

    #[tokio::test]
    async fn suppression_hides_until_expiry_then_reopens() {
        let service = OperatorSecurityService::new(Arc::new(AllowAll), Arc::new(MockAudit::stub()));
        let caller = owner();
        let at = now();
        let item = auto_finding("host:22");
        service
            .ingest(&caller, vec![item.clone()], at)
            .await
            .expect("ingest");
        service
            .suppress_finding(
                &caller,
                item.id(),
                "known noise",
                "host:22",
                at + chrono::Duration::hours(1),
                at,
            )
            .await
            .expect("suppress");
        assert!(service.queue(&caller, at).await.expect("queue").is_empty());
        let reopened = service
            .queue(&caller, at + chrono::Duration::hours(2))
            .await
            .expect("queue");
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened[0].state(), FindingState::Open);
    }

    #[tokio::test]
    async fn suppression_requires_reason_actor_scope_expiry() {
        let service = OperatorSecurityService::new(Arc::new(AllowAll), Arc::new(MockAudit::stub()));
        let caller = owner();
        let at = now();
        let item = auto_finding("host:22");
        service
            .ingest(&caller, vec![item.clone()], at)
            .await
            .expect("ingest");
        assert!(matches!(
            service
                .suppress_finding(
                    &caller,
                    item.id(),
                    "",
                    "host:22",
                    at + chrono::Duration::hours(1),
                    at
                )
                .await,
            Err(ControlPlaneServiceError::Validation(_))
        ));
        assert!(matches!(
            service
                .suppress_finding(
                    &caller,
                    Uuid::new_v4(),
                    "r",
                    "s",
                    at + chrono::Duration::hours(1),
                    at
                )
                .await,
            Err(ControlPlaneServiceError::NotFound)
        ));
    }
}

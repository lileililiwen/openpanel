//! Deployment-adapter service: idempotent action lifecycle,
//! evidence persistence, and audit fan-out.
//!
//! The service is the single entry point that the API, the CLI,
//! and the integration tests share. It owns:
//! - the registered adapter set (`Arc<dyn DeploymentAdapter>`),
//! - the in-memory evidence cache used for idempotency replay,
//! - the audit fan-out (`openpanel_core::AuditService`).
//!
//! The service NEVER returns a host path, a secret value, or a
//! transport-specific command. Diagnostics are scrubbed by
//! [`openpanel_domain::deployment_adapters::redact_diagnostic`]
//! and audit metadata is redacted by the canonical
//! `openpanel_core::audit::redact_metadata` allowlist.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService, audit::redact_metadata};
use openpanel_domain::{
    AdapterManifest, DeploymentAction, DeploymentAdapter, DeploymentAdapterError,
    DeploymentEvidence, DeploymentPlan, DeploymentState, IdempotencyDecision, Role, User,
    decide_replay, validate_plan,
};
use uuid::Uuid;

use super::types::DeploymentServiceError;

/// In-memory deployment-adapter service. Idempotency and evidence
/// are kept in `Mutex` maps; the production wiring is a thin
/// swap-in for an SQLite-backed repository when the bounded
/// context grows.
pub struct DeploymentAdapterService {
    adapters: HashMap<String, Arc<dyn DeploymentAdapter>>,
    evidence: Mutex<HashMap<Uuid, DeploymentEvidence>>,
    audit: Arc<dyn AuditService>,
}

impl DeploymentAdapterService {
    /// Compose a service with a registry of adapters and an audit
    /// sink. Adapters are keyed by their manifest id.
    pub fn new(adapters: Vec<Arc<dyn DeploymentAdapter>>, audit: Arc<dyn AuditService>) -> Self {
        let mut map = HashMap::new();
        for adapter in adapters {
            map.insert(adapter.manifest().id().to_string(), adapter);
        }
        Self {
            adapters: map,
            evidence: Mutex::new(HashMap::new()),
            audit,
        }
    }

    /// Owner/Admin gate (defense in depth; handlers gate again).
    fn require_operator(user: &User) -> Result<(), DeploymentServiceError> {
        if matches!(user.role(), Role::Owner | Role::Admin) {
            Ok(())
        } else {
            Err(DeploymentServiceError::Forbidden)
        }
    }

    /// List the registered adapter manifests. Sorted by id.
    pub fn list_manifests(&self) -> Vec<AdapterManifest> {
        let mut manifests: Vec<AdapterManifest> = self
            .adapters
            .values()
            .map(|a| a.manifest().clone())
            .collect();
        manifests.sort_by(|left, right| left.id().cmp(right.id()));
        manifests
    }

    /// Look up a single adapter manifest by id.
    pub fn manifest(&self, id: &str) -> Option<AdapterManifest> {
        self.adapters.get(id).map(|a| a.manifest().clone())
    }

    /// Look up the cached terminal evidence for a plan. Returns
    /// `None` when no record exists or when the cached record is
    /// not in a terminal state. Exposed for callers that need to
    /// check idempotency before deciding to record an audit
    /// event.
    pub fn cached_evidence(
        &self,
        plan: &DeploymentPlan,
    ) -> Result<Option<DeploymentEvidence>, DeploymentServiceError> {
        let Some(key) = plan.operation_key() else {
            return Ok(None);
        };
        let guard = self
            .evidence
            .lock()
            .map_err(|_| DeploymentServiceError::Internal)?;
        let cached = guard
            .values()
            .find(|evidence| {
                evidence.state().is_terminal()
                    && evidence.action() == plan.action()
                    && evidence.target() == plan.target()
                    && evidence.release_digest() == plan.release_digest()
                    && evidence
                        .operation_key()
                        .is_some_and(|candidate| candidate.value() == key.value())
            })
            .cloned();
        Ok(cached)
    }

    /// Run a plan end-to-end: validate, decide idempotency replay,
    /// execute, record evidence, fan out audit. Dry-run plans
    /// produce a synthetic terminal evidence record without
    /// mutating and without invoking the adapter.
    pub async fn run(
        &self,
        caller: &User,
        plan: DeploymentPlan,
    ) -> Result<DeploymentEvidence, DeploymentServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, &plan, "run");
        })?;
        let manifest = self
            .adapters
            .get(plan.adapter())
            .ok_or_else(|| DeploymentServiceError::AdapterNotFound(plan.adapter().to_string()))?
            .manifest()
            .clone();
        validate_plan(&plan, &manifest).map_err(Self::propagate_validation)?;
        if plan.action() == DeploymentAction::Rollback && !manifest.supports_rollback() {
            return Err(DeploymentServiceError::RollbackUnsupported);
        }
        if plan.dry_run() {
            let evidence = build_dry_run_evidence(&plan, &manifest)?;
            self.record_evidence(evidence.clone())?;
            self.audit_success(caller, &plan, &evidence);
            return Ok(evidence);
        }
        let cached = self.cached_evidence(&plan)?;
        let decision = decide_replay(&plan, cached.as_ref());
        let adapter = self
            .adapters
            .get(plan.adapter())
            .ok_or_else(|| DeploymentServiceError::AdapterNotFound(plan.adapter().to_string()))?
            .clone();
        let evidence = adapter.execute(&plan, cached.as_ref());
        if decision == IdempotencyDecision::Replay {
            // Replay: do not re-record; just return the original.
            return Ok(evidence);
        }
        self.record_evidence(evidence.clone())?;
        if evidence.state() == DeploymentState::Ready {
            self.audit_success(caller, &plan, &evidence);
        } else if evidence.state().is_failure() {
            self.audit_failure(caller, &plan, &evidence);
        } else {
            // Non-terminal evidence is unexpected from a conformant
            // adapter; record as a failure so operators can route.
            self.audit_failure(caller, &plan, &evidence);
        }
        Ok(evidence)
    }

    /// Pure idempotency helper exposed for the API and CLI so they
    /// can surface a `Replay` decision before invoking the
    /// adapter. Returns the cached evidence when the decision is
    /// `Replay`.
    pub fn replay_candidate(
        &self,
        plan: &DeploymentPlan,
    ) -> Result<Option<DeploymentEvidence>, DeploymentServiceError> {
        let cached = self.cached_evidence(plan)?;
        match decide_replay(plan, cached.as_ref()) {
            IdempotencyDecision::Replay => Ok(cached),
            IdempotencyDecision::Rerun => Ok(None),
        }
    }

    fn propagate_validation(err: DeploymentAdapterError) -> DeploymentServiceError {
        match err {
            DeploymentAdapterError::UnsupportedAction(action) => {
                DeploymentServiceError::UnsupportedAction(action)
            }
            other => DeploymentServiceError::Validation(other.to_string()),
        }
    }

    fn record_evidence(&self, evidence: DeploymentEvidence) -> Result<(), DeploymentServiceError> {
        let mut guard = self
            .evidence
            .lock()
            .map_err(|_| DeploymentServiceError::Internal)?;
        guard.insert(evidence.id(), evidence);
        Ok(())
    }

    fn audit_success(&self, caller: &User, plan: &DeploymentPlan, evidence: &DeploymentEvidence) {
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
        )
        .target(plan.target())
        .metadata(redact_metadata(&serde_json::json!({
            "adapter": plan.adapter(),
            "action": plan.action().as_str(),
            "release": plan.release_digest(),
            "state": evidence.state().as_str(),
            "evidence_id": evidence.id().to_string(),
        })));
        let audit = self.audit.clone();
        // The `record` call is async; the test harness and the
        // HTTP wiring drive it through a `tokio::spawn` or an
        // awaited call site. We detach a `tokio::spawn` here so
        // the service stays sync-friendly.
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }

    fn audit_failure(&self, caller: &User, plan: &DeploymentPlan, evidence: &DeploymentEvidence) {
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::SoftwareChanged,
            AuditOutcome::Failure,
        )
        .target(plan.target())
        .metadata(redact_metadata(&serde_json::json!({
            "adapter": plan.adapter(),
            "action": plan.action().as_str(),
            "release": plan.release_digest(),
            "state": evidence.state().as_str(),
            "diagnostic": evidence.diagnostic(),
            "evidence_id": evidence.id().to_string(),
        })));
        let audit = self.audit.clone();
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }

    fn audit_denied(&self, caller: &User, plan: &DeploymentPlan, intent: &str) {
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::PermissionDenied,
            AuditOutcome::Denied,
        )
        .target(plan.target())
        .metadata(redact_metadata(&serde_json::json!({
            "adapter": plan.adapter(),
            "action": plan.action().as_str(),
            "intent": intent,
        })));
        let audit = self.audit.clone();
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }
}

fn build_dry_run_evidence(
    plan: &DeploymentPlan,
    manifest: &AdapterManifest,
) -> Result<DeploymentEvidence, DeploymentServiceError> {
    let started_at = Utc::now();
    let completed_at = started_at;
    // Validation already ran; if we reach this point the plan is
    // valid for the manifest, so a dry run always reports `Ready`
    // (the contract guarantees no mutation occurred).
    let _ = manifest;
    DeploymentEvidence::new(
        plan.target(),
        plan.adapter(),
        plan.action(),
        plan.operation_key().cloned(),
        plan.release_digest(),
        DeploymentState::Ready,
        started_at,
        completed_at,
        "dry-run; no mutation",
    )
    .map_err(|err| DeploymentServiceError::Validation(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpanel_core::audit::NoopAuditService;
    use openpanel_domain::OperationKey;

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

    fn compose_manifest() -> AdapterManifest {
        AdapterManifest::new(
            "test-adapter",
            "Test Adapter",
            openpanel_domain::RUNTIME_CONTRACT_VERSION,
            "test://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Rollback,
            ],
            true,
            openpanel_domain::HealthMethod::HttpGet {
                url: "http://127.0.0.1:8080/healthz".into(),
                expect_body_contains: Some("ok".into()),
            },
            vec!["agent-token".into()],
        )
        .unwrap()
    }

    /// Stub adapter used in the service unit tests. The execute
    /// method always returns a `Ready` evidence record; the
    /// service treats that as a successful deploy.
    struct StubAdapter {
        manifest: AdapterManifest,
    }

    impl DeploymentAdapter for StubAdapter {
        fn manifest(&self) -> &AdapterManifest {
            &self.manifest
        }

        fn execute(
            &self,
            plan: &DeploymentPlan,
            cached: Option<&DeploymentEvidence>,
        ) -> DeploymentEvidence {
            if let Some(cached) = cached {
                return cached.clone();
            }
            let started_at = Utc::now();
            let completed_at = started_at + chrono::Duration::seconds(1);
            DeploymentEvidence::new(
                plan.target(),
                plan.adapter(),
                plan.action(),
                plan.operation_key().cloned(),
                plan.release_digest(),
                DeploymentState::Ready,
                started_at,
                completed_at,
                "stub: ok",
            )
            .unwrap()
        }
    }

    fn service() -> DeploymentAdapterService {
        let manifest = compose_manifest();
        let adapter: Arc<dyn DeploymentAdapter> = Arc::new(StubAdapter { manifest });
        DeploymentAdapterService::new(vec![adapter], Arc::new(NoopAuditService))
    }

    fn build_plan(action: DeploymentAction) -> DeploymentPlan {
        let key = OperationKey::new("test://web-01", action, "v1").unwrap();
        DeploymentPlan::new(
            "test://web-01",
            action,
            if action.is_mutating() {
                Some(key)
            } else {
                None
            },
            "sha256:deadbeef",
            vec![],
            "test-adapter",
            "owner",
            false,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn run_rejects_non_operator() {
        let service = service();
        let plan = build_plan(DeploymentAction::Deploy);
        let result = service.run(&member(), plan).await;
        assert!(matches!(result, Err(DeploymentServiceError::Forbidden)));
    }

    #[tokio::test]
    async fn run_rejects_unknown_adapter() {
        let service = service();
        let plan = DeploymentPlan::new(
            "test://web-01",
            DeploymentAction::Status,
            None,
            "sha256:deadbeef",
            vec![],
            "no-such-adapter",
            "owner",
            false,
        )
        .unwrap();
        let result = service.run(&owner(), plan).await;
        assert!(matches!(
            result,
            Err(DeploymentServiceError::AdapterNotFound(_))
        ));
    }

    #[tokio::test]
    async fn dry_run_records_ready_evidence_without_executing() {
        let service = service();
        let mut plan = build_plan(DeploymentAction::Deploy);
        plan = DeploymentPlan::new(
            plan.target(),
            plan.action(),
            plan.operation_key().cloned(),
            plan.release_digest(),
            plan.secret_refs().to_vec(),
            plan.adapter(),
            plan.actor(),
            true,
        )
        .unwrap();
        let evidence = service.run(&owner(), plan).await.unwrap();
        assert_eq!(evidence.state(), DeploymentState::Ready);
        assert!(evidence.diagnostic().contains("dry-run"));
    }

    #[tokio::test]
    async fn run_replay_returns_cached_evidence() {
        let service = service();
        let plan = build_plan(DeploymentAction::Deploy);
        let first = service.run(&owner(), plan.clone()).await.unwrap();
        assert_eq!(first.state(), DeploymentState::Ready);
        let second = service.run(&owner(), plan).await.unwrap();
        assert_eq!(first.id(), second.id());
    }

    #[tokio::test]
    async fn list_manifests_returns_sorted() {
        let service = service();
        let manifests = service.list_manifests();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].id(), "test-adapter");
    }

    #[tokio::test]
    async fn replay_candidate_returns_none_for_first_run() {
        let service = service();
        let plan = build_plan(DeploymentAction::Deploy);
        let candidate = service.replay_candidate(&plan).unwrap();
        assert!(candidate.is_none());
    }
}

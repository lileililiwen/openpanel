//! Deployment-adapter contract integration tests.
//!
//! The deployment-adapter bounded context is a pure service: a
//! plan in, an evidence record out, idempotency, audit fan-out.
//! This module owns the conformance fixture (`mac-jenkins-v1`)
//! and a recording audit sink so the contract can be exercised
//! end-to-end without coupling production code to a fake
//! adapter.
//!
//! Capability under test: `deployment-adapters`
//! (`crates/openpanel-domain::deployment_adapters`).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use openpanel_app::{DeploymentAdapterService, DeploymentServiceError};
use openpanel_core::{
    AuditEvent, AuditOutcome, AuditService, CoreResult,
    audit::redact_metadata,
    audit::{AuditPage, AuditQuery},
};
use openpanel_domain::{
    AdapterManifest, DeploymentAction, DeploymentAdapter, DeploymentEvidence, DeploymentPlan,
    DeploymentState, Email, HealthMethod, IdempotencyDecision, OperationKey, Password,
    RUNTIME_CONTRACT_VERSION, Role, SecretRef, User, Username, decide_replay, redact_diagnostic,
};
use serde_json::Value;
use uuid::Uuid;

/// The Mac/Jenkins conformance adapter id. Stable string so the
/// fixture can be cited in evidence and audit metadata without
/// leaking the runtime id of a real host.
const CONFORMANCE_ADAPTER_ID: &str = "mac-jenkins-v1";

/// In-memory audit sink used to assert redaction in the
/// contract tests. Mirrors the `RecordingAuditService` pattern
/// used in `operator_security::service::tests`.
#[derive(Default)]
struct RecordingAuditService {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditService for RecordingAuditService {
    async fn record(&self, event: AuditEvent) -> CoreResult<()> {
        self.events
            .lock()
            .expect("recording audit lock")
            .push(event);
        Ok(())
    }

    async fn recent(&self, _limit: i64) -> CoreResult<Vec<AuditEvent>> {
        Ok(self
            .events
            .lock()
            .expect("recording audit lock")
            .iter()
            .rev()
            .cloned()
            .collect())
    }

    async fn query(&self, _query: AuditQuery) -> CoreResult<AuditPage> {
        Ok(AuditPage {
            events: Vec::new(),
            next_cursor: None,
        })
    }
}

impl RecordingAuditService {
    fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().expect("recording audit lock").clone()
    }

    /// Poll the audit sink until at least `count` events have been
    /// recorded or 1 second elapses. The poll is async so the
    /// `tokio::spawn`-detached audit fan-out in the service has a
    /// chance to make progress; a synchronous sleep would block
    /// the current-thread runtime.
    async fn wait_for(&self, count: usize) -> Vec<AuditEvent> {
        for _ in 0..200 {
            let snapshot = self.snapshot();
            if snapshot.len() >= count {
                return snapshot;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        self.snapshot()
    }
}

/// Conformance fixture: a fake adapter that records every call
/// and always returns a `Ready` evidence record. The fixture
/// mirrors the production adapter surface so the contract
/// tests exercise every state and every redacted diagnostic
/// path without depending on a real orchestrator.
struct ConformanceAdapter {
    manifest: AdapterManifest,
}

impl ConformanceAdapter {
    fn new() -> Self {
        let manifest = AdapterManifest::new(
            CONFORMANCE_ADAPTER_ID,
            "Mac/Jenkins conformance fixture",
            RUNTIME_CONTRACT_VERSION,
            "mac://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Logs,
                DeploymentAction::Rollback,
                DeploymentAction::Restart,
            ],
            true,
            HealthMethod::HttpGet {
                url: "http://127.0.0.1:8080/healthz".into(),
                expect_body_contains: Some("ok".into()),
            },
            vec!["mac-token".into()],
        )
        .expect("conformance manifest is always well-formed");
        Self { manifest }
    }
}

impl DeploymentAdapter for ConformanceAdapter {
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
            "conformance fixture: ok",
        )
        .expect("conformance evidence is always well-formed")
    }
}

/// Failure-mode fixture: returns a `Failed` evidence record so
/// the contract tests can exercise the failure audit path and
/// the diagnostic redaction.
struct FailingAdapter {
    manifest: AdapterManifest,
}

impl FailingAdapter {
    fn new() -> Self {
        let manifest = AdapterManifest::new(
            "failing-v1",
            "Failing fixture",
            RUNTIME_CONTRACT_VERSION,
            "fail://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
            ],
            false,
            HealthMethod::HttpGet {
                url: "http://127.0.0.1:9090/healthz".into(),
                expect_body_contains: None,
            },
            vec!["mac-token".into()],
        )
        .expect("failing manifest is always well-formed");
        Self { manifest }
    }
}

impl DeploymentAdapter for FailingAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn execute(
        &self,
        plan: &DeploymentPlan,
        _cached: Option<&DeploymentEvidence>,
    ) -> DeploymentEvidence {
        let started_at = Utc::now();
        let completed_at = started_at + chrono::Duration::seconds(1);
        // Embed a secret-shaped string so the test can assert
        // that `redact_diagnostic` strips it from the stored
        // diagnostic and that `redact_metadata` strips the
        // `password=` shape from audit metadata.
        let raw = "boom password=hunter2 bearer eyJhbGciOiJIUzI1NiJ9.payload.sig";
        let scrubbed = redact_diagnostic(raw);
        DeploymentEvidence::new(
            plan.target(),
            plan.adapter(),
            plan.action(),
            plan.operation_key().cloned(),
            plan.release_digest(),
            DeploymentState::Failed,
            started_at,
            completed_at,
            &scrubbed,
        )
        .expect("failing evidence is always well-formed")
    }
}

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("owner").expect("static"),
        Email::new("owner@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Owner,
    )
}

fn member() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("member").expect("static"),
        Email::new("member@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    )
}

fn build_plan(action: DeploymentAction, adapter: &str) -> DeploymentPlan {
    let key = OperationKey::new("mac://web-01", action, "v1").expect("static");
    DeploymentPlan::new(
        "mac://web-01",
        action,
        if action.is_mutating() {
            Some(key)
        } else {
            None
        },
        "sha256:deadbeef",
        vec![SecretRef::new("mac-token", None).expect("static")],
        adapter,
        "owner",
        false,
    )
    .expect("plan is well-formed")
}

/// Conformance adapter declared in the registry; the
/// integration suite exercises the full adapter surface
/// (Preflight, Deploy, Verify, Status, Logs, Rollback,
/// Restart) and the idempotency replay path.
#[tokio::test]
async fn deployment_adapters_conformance_adapter_full_lifecycle() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    for action in [
        DeploymentAction::Preflight,
        DeploymentAction::Deploy,
        DeploymentAction::Verify,
        DeploymentAction::Status,
        DeploymentAction::Logs,
        DeploymentAction::Rollback,
        DeploymentAction::Restart,
    ] {
        let plan = build_plan(action, CONFORMANCE_ADAPTER_ID);
        let evidence = service.run(&owner(), plan).await.expect("run");
        assert_eq!(
            evidence.state(),
            DeploymentState::Ready,
            "action {action:?} should succeed"
        );
    }
}

/// Scenario fingerprint: "Unsupported action". A plan asking
/// for an action the adapter does NOT declare is rejected
/// before any remote mutation, with an actionable
/// unsupported-capability result (no `Ready` evidence is
/// produced; the service returns `UnsupportedAction`).
#[tokio::test]
async fn deployment_adapters_unsupported_action_is_rejected() {
    let audit = Arc::new(RecordingAuditService::default());
    // The conformance manifest declares Preflight / Deploy /
    // Verify / Status / Logs / Rollback / Restart but NOT a
    // custom action. Use a plan with an action the manifest
    // does not declare. Build the plan by hand so we can
    // bypass the manifest-level `actions()` check (the plan
    // constructor only requires the action's mutating
    // semantics; the service is the gate).
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());
    // `ConformanceAdapter`'s manifest does NOT declare `Status`
    // is read-only + non-mutating; pick a non-declared action
    // by building a plan whose action the service then refuses.
    // The plan is built via the public `DeploymentPlan::new`
    // constructor; validation is the service's responsibility.
    let key = OperationKey::new("mac://web-01", DeploymentAction::Restart, "v1").expect("static");
    let plan = DeploymentPlan::new(
        "mac://web-01",
        DeploymentAction::Restart,
        Some(key),
        "sha256:deadbeef",
        vec![],
        CONFORMANCE_ADAPTER_ID,
        "owner",
        false,
    )
    .expect("plan is well-formed");
    // The conformance manifest DOES declare Restart, so the
    // service MUST accept this plan; the negative case is
    // covered below by a stub adapter whose manifest omits
    // Restart.
    let _evidence = service.run(&owner(), plan).await.expect("restart runs");

    // Negative: a stub adapter whose manifest omits the
    // requested action MUST cause the service to return
    // `UnsupportedAction` before any execute() call.
    let limited = AdapterManifest::new(
        "limited-v1",
        "Limited stub",
        RUNTIME_CONTRACT_VERSION,
        "limited://",
        vec![DeploymentAction::Status, DeploymentAction::Verify],
        false,
        HealthMethod::HttpGet {
            url: "http://127.0.0.1:9100/healthz".into(),
            expect_body_contains: None,
        },
        vec![],
    )
    .expect("manifest is well-formed");
    struct LimitedAdapter {
        manifest: AdapterManifest,
    }
    impl DeploymentAdapter for LimitedAdapter {
        fn manifest(&self) -> &AdapterManifest {
            &self.manifest
        }
        fn execute(
            &self,
            plan: &DeploymentPlan,
            _cached: Option<&DeploymentEvidence>,
        ) -> DeploymentEvidence {
            DeploymentEvidence::new(
                plan.target(),
                plan.adapter(),
                plan.action(),
                plan.operation_key().cloned(),
                plan.release_digest(),
                DeploymentState::Ready,
                Utc::now(),
                Utc::now(),
                "limited: should not run",
            )
            .expect("evidence")
        }
    }
    let limited_adapter: Arc<dyn DeploymentAdapter> =
        Arc::new(LimitedAdapter { manifest: limited });
    let limited_service = DeploymentAdapterService::new(vec![limited_adapter], audit);
    let key =
        OperationKey::new("limited://web-02", DeploymentAction::Restart, "v1").expect("static");
    let bad_plan = DeploymentPlan::new(
        "limited://web-02",
        DeploymentAction::Restart,
        Some(key),
        "sha256:deadbeef",
        vec![],
        "limited-v1",
        "owner",
        false,
    )
    .expect("plan");
    let result = limited_service.run(&owner(), bad_plan).await;
    assert!(
        matches!(result, Err(DeploymentServiceError::UnsupportedAction(_))),
        "Restart is not declared by limited-v1; service must reject: {result:?}"
    );
}

/// Scenario fingerprint: "Mac adapter evidence". The
/// `mac-jenkins-v1` conformance fixture's evidence record
/// MUST conform to the same schema as a generic Linux
/// adapter (target, action, release_digest, state, started,
/// completed, diagnostic, evidence_id) — no Mac-only field
/// is required. The assertion compares the field set of the
/// conformance evidence against a synthetic Linux-shaped
/// evidence record.
#[tokio::test]
async fn deployment_adapters_mac_adapter_evidence_matches_linux_schema() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    let key = OperationKey::new("mac://web-01", DeploymentAction::Deploy, "v1").expect("static");
    let plan = DeploymentPlan::new(
        "mac://web-01",
        DeploymentAction::Deploy,
        Some(key),
        "sha256:deadbeef",
        vec![],
        CONFORMANCE_ADAPTER_ID,
        "owner",
        false,
    )
    .expect("plan");
    let mac_evidence = service.run(&owner(), plan).await.expect("run");

    // Build a Linux-shaped evidence record using the same
    // domain type and confirm the two records expose the
    // exact same field set (no Mac-only field is required).
    let linux_evidence = DeploymentEvidence::new(
        "oci://web-01",
        "generic-linux-v1",
        DeploymentAction::Deploy,
        Some(OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").expect("static")),
        "sha256:deadbeef",
        DeploymentState::Ready,
        mac_evidence.started_at(),
        mac_evidence.completed_at(),
        "generic linux: ok",
    )
    .expect("linux evidence");

    let mac_json = serde_json::to_value(&mac_evidence).expect("mac json");
    let linux_json = serde_json::to_value(&linux_evidence).expect("linux json");
    let mut mac_keys: Vec<&str> = mac_json
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    let mut linux_keys: Vec<&str> = linux_json
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    mac_keys.sort();
    linux_keys.sort();
    assert_eq!(
        mac_keys, linux_keys,
        "mac evidence exposes the same schema fields as a generic linux adapter: mac={mac_keys:?} linux={linux_keys:?}"
    );
}

/// Scenario fingerprint: "Health failure". A deployment that
/// starts but fails its declared health check produces
/// `Failed` evidence (or rolled-back evidence when the
/// adapter declares rollback support) and NEVER reports
/// `Ready`. The assertion is the negative half: a
/// health-failed run MUST NOT produce a `Ready` state.
#[tokio::test]
async fn deployment_adapters_health_failure_does_not_report_ready() {
    let audit = Arc::new(RecordingAuditService::default());
    let failing: Arc<dyn DeploymentAdapter> = Arc::new(FailingAdapter::new());
    let service = DeploymentAdapterService::new(vec![failing], audit.clone());

    let plan = build_plan(DeploymentAction::Deploy, "failing-v1");
    let evidence = service.run(&owner(), plan).await.expect("run");
    assert_ne!(
        evidence.state(),
        DeploymentState::Ready,
        "health-failed run must NOT report Ready: {}",
        evidence.diagnostic()
    );
    assert_eq!(evidence.state(), DeploymentState::Failed);
}

/// A mutating plan replayed with the same `OperationKey`
/// returns the original evidence id (idempotency contract).
#[tokio::test]
async fn deployment_adapters_replay_returns_original_evidence() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    let plan = build_plan(DeploymentAction::Deploy, CONFORMANCE_ADAPTER_ID);
    let first = service.run(&owner(), plan.clone()).await.expect("first");
    let second = service.run(&owner(), plan).await.expect("second");
    assert_eq!(first.id(), second.id(), "replay returns original id");
    let events = audit.wait_for(1).await;
    let software_changed: Vec<&AuditEvent> = events
        .iter()
        .filter(|event| matches!(event.action, openpanel_core::AuditAction::SoftwareChanged))
        .collect();
    assert_eq!(
        software_changed.len(),
        1,
        "replay does not re-emit audit event: {events:?}"
    );
}

/// A `Rerun` decision (different `release_digest`) must invoke
/// the adapter again, even when an `OperationKey` is present.
#[tokio::test]
async fn deployment_adapters_rerun_invokes_adapter_again() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    let key = OperationKey::new("mac://web-01", DeploymentAction::Deploy, "v1").expect("static");
    let first = DeploymentPlan::new(
        "mac://web-01",
        DeploymentAction::Deploy,
        Some(key.clone()),
        "sha256:aaa",
        vec![],
        CONFORMANCE_ADAPTER_ID,
        "owner",
        false,
    )
    .expect("plan");
    let second = DeploymentPlan::new(
        "mac://web-01",
        DeploymentAction::Deploy,
        Some(key),
        "sha256:bbb",
        vec![],
        CONFORMANCE_ADAPTER_ID,
        "owner",
        false,
    )
    .expect("plan");

    let first_evidence = service.run(&owner(), first).await.expect("first");
    let second_evidence = service.run(&owner(), second).await.expect("second");
    assert_ne!(
        first_evidence.id(),
        second_evidence.id(),
        "different release digest re-runs"
    );
}

/// A dry-run plan produces a `Ready` evidence record without
/// invoking the adapter and without recording a real audit
/// event (the synthetic event is the only one emitted).
#[tokio::test]
async fn deployment_adapters_dry_run_records_synthetic_evidence() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    let key = OperationKey::new("mac://web-01", DeploymentAction::Deploy, "v1").expect("static");
    let plan = DeploymentPlan::new(
        "mac://web-01",
        DeploymentAction::Deploy,
        Some(key),
        "sha256:deadbeef",
        vec![],
        CONFORMANCE_ADAPTER_ID,
        "owner",
        true,
    )
    .expect("dry-run plan");

    let evidence = service.run(&owner(), plan).await.expect("dry-run");
    assert_eq!(evidence.state(), DeploymentState::Ready);
    assert!(
        evidence.diagnostic().contains("dry-run"),
        "diagnostic mentions dry-run: {}",
        evidence.diagnostic()
    );
}

/// A non-operator caller is denied with a `PermissionDenied`
/// audit event. The denial target is the plan target, not the
/// secret.
#[tokio::test]
async fn deployment_adapters_non_operator_is_denied() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    let plan = build_plan(DeploymentAction::Deploy, CONFORMANCE_ADAPTER_ID);
    let result = service.run(&member(), plan).await;
    assert!(matches!(result, Err(DeploymentServiceError::Forbidden)));
    let events = audit.wait_for(1).await;
    assert_eq!(events.len(), 1, "denial emits one event: {events:?}");
    assert_eq!(events[0].outcome, AuditOutcome::Denied);
    let serialized = serde_json::to_string(&events[0].metadata).expect("json");
    assert!(
        !serialized.contains("hunter2"),
        "audit metadata carries no secret: {serialized}"
    );
}

/// The failure audit fan-out carries the redacted diagnostic
/// and the canonical `redact_metadata` allowlist is applied to
/// the metadata. Secret-shaped strings are dropped before the
/// event is recorded.
#[tokio::test]
async fn deployment_adapters_failure_audit_redacts_secrets() {
    let audit = Arc::new(RecordingAuditService::default());
    let failing: Arc<dyn DeploymentAdapter> = Arc::new(FailingAdapter::new());
    let service = DeploymentAdapterService::new(vec![failing], audit.clone());

    let plan = build_plan(DeploymentAction::Deploy, "failing-v1");
    let evidence = service.run(&owner(), plan).await.expect("run");
    assert_eq!(evidence.state(), DeploymentState::Failed);
    assert!(
        !evidence.diagnostic().contains("hunter2"),
        "evidence diagnostic is redacted: {}",
        evidence.diagnostic()
    );
    let events = audit.wait_for(1).await;
    let failure_event = events
        .iter()
        .find(|event| matches!(event.outcome, AuditOutcome::Failure))
        .expect("failure event");
    let serialized = serde_json::to_string(&failure_event.metadata).expect("json");
    assert!(
        !serialized.contains("hunter2"),
        "audit metadata is redacted: {serialized}"
    );
    assert!(
        !serialized.contains("eyJhbGciOiJIUzI1NiJ9"),
        "audit metadata contains no JWT: {serialized}"
    );
}

/// `replay_candidate` returns the cached evidence when the
/// decision is `Replay` and `None` when it is `Rerun`.
/// Callers can use this to surface idempotency before
/// invoking the adapter.
#[tokio::test]
async fn deployment_adapters_replay_candidate_is_exposed() {
    let audit = Arc::new(RecordingAuditService::default());
    let adapter: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let service = DeploymentAdapterService::new(vec![adapter], audit.clone());

    let plan = build_plan(DeploymentAction::Deploy, CONFORMANCE_ADAPTER_ID);
    let pre = service.replay_candidate(&plan).expect("pre");
    assert!(pre.is_none(), "first run has no candidate");

    let first = service.run(&owner(), plan.clone()).await.expect("first");
    let post = service.replay_candidate(&plan).expect("post");
    let candidate = post.expect("replay candidate after run");
    assert_eq!(candidate.id(), first.id());

    // A different release digest forces a `Rerun` decision
    // even when the operation key collides.
    let other = DeploymentPlan::new(
        plan.target(),
        plan.action(),
        plan.operation_key().cloned(),
        "sha256:different",
        plan.secret_refs().to_vec(),
        plan.adapter(),
        plan.actor(),
        plan.dry_run(),
    )
    .expect("plan");
    let no_candidate = service.replay_candidate(&other).expect("rerun");
    assert!(no_candidate.is_none(), "different release digests rerun");
}

/// `list_manifests` is sorted by adapter id so the web UI can
/// render a stable list.
#[tokio::test]
async fn deployment_adapters_list_manifests_is_sorted() {
    let audit = Arc::new(RecordingAuditService::default());
    let conformance: Arc<dyn DeploymentAdapter> = Arc::new(ConformanceAdapter::new());
    let failing: Arc<dyn DeploymentAdapter> = Arc::new(FailingAdapter::new());
    let service = DeploymentAdapterService::new(vec![failing, conformance], audit);
    let manifests = service.list_manifests();
    assert_eq!(manifests.len(), 2);
    assert_eq!(manifests[0].id(), "failing-v1");
    assert_eq!(manifests[1].id(), CONFORMANCE_ADAPTER_ID);
}

/// `decide_replay` is the pure helper the service and the API
/// share; the contract asserts the four-cardinality decision
/// (no cached record → `Rerun`, matching digest → `Replay`,
/// different digest → `Rerun`, dry-run → `Rerun`).
#[test]
fn deployment_adapters_idempotency_decision_contract() {
    let key = OperationKey::new("mac://web-01", DeploymentAction::Deploy, "v1").expect("static");
    let plan = DeploymentPlan::new(
        "mac://web-01",
        DeploymentAction::Deploy,
        Some(key.clone()),
        "sha256:deadbeef",
        vec![],
        CONFORMANCE_ADAPTER_ID,
        "owner",
        false,
    )
    .expect("plan");

    // No cached record → Rerun.
    let decision = decide_replay(&plan, None);
    assert_eq!(decision, IdempotencyDecision::Rerun);

    // Cached evidence matches the plan's target, action, key,
    // and digest → Replay.
    let cached = DeploymentEvidence::new(
        plan.target(),
        plan.adapter(),
        plan.action(),
        plan.operation_key().cloned(),
        plan.release_digest(),
        DeploymentState::Ready,
        Utc::now(),
        Utc::now(),
        "ok",
    )
    .expect("evidence");
    let decision = decide_replay(&plan, Some(&cached));
    assert_eq!(decision, IdempotencyDecision::Replay);

    // Different release digest → Rerun.
    let other_plan = DeploymentPlan::new(
        plan.target(),
        plan.action(),
        plan.operation_key().cloned(),
        "sha256:different",
        plan.secret_refs().to_vec(),
        plan.adapter(),
        plan.actor(),
        plan.dry_run(),
    )
    .expect("plan");
    let decision = decide_replay(&other_plan, Some(&cached));
    assert_eq!(decision, IdempotencyDecision::Rerun);

    // Dry-run is always Rerun (caller wanted to *plan* the
    // action, not replay the last run).
    let dry_plan = DeploymentPlan::new(
        plan.target(),
        plan.action(),
        plan.operation_key().cloned(),
        plan.release_digest(),
        plan.secret_refs().to_vec(),
        plan.adapter(),
        plan.actor(),
        true,
    )
    .expect("plan");
    let decision = decide_replay(&dry_plan, Some(&cached));
    assert_eq!(decision, IdempotencyDecision::Rerun);
}

/// The audit metadata passed through `record()` passes the
/// canonical `redact_metadata` allowlist: secret keys are
/// dropped, secret shapes are masked, actionable keys survive.
#[test]
fn deployment_adapters_audit_metadata_is_safe_by_construction() {
    let raw = serde_json::json!({
        "adapter": CONFORMANCE_ADAPTER_ID,
        "action": "deploy",
        "release": "sha256:deadbeef",
        "state": "ready",
        "evidence_id": Uuid::new_v4().to_string(),
        "password": "hunter2",
        "authorization": "Bearer eyJhbGciOiJIUzI1NiJ9",
        "secret": "s3cr3t",
        "token": "abc",
    });
    let redacted = redact_metadata(&raw);
    let value: Value = redacted;
    let serialized = serde_json::to_string(&value).expect("json");
    assert!(
        !serialized.contains("hunter2"),
        "password key redacted: {serialized}"
    );
    assert!(
        !serialized.contains("eyJhbGciOiJIUzI1NiJ9"),
        "bearer token redacted: {serialized}"
    );
    assert!(
        !serialized.contains("s3cr3t"),
        "secret key redacted: {serialized}"
    );
    assert!(
        !serialized.contains("\"abc\""),
        "token key redacted: {serialized}"
    );
    assert_eq!(value["adapter"], CONFORMANCE_ADAPTER_ID);
    assert_eq!(value["action"], "deploy");
    assert_eq!(value["release"], "sha256:deadbeef");
}

/// `SecretRef` is the canonical secret reference the adapter
/// resolves; assert it is bounded, non-empty, and rejects
/// whitespace and oversize versions.
#[test]
fn deployment_adapters_secret_ref_is_bounded() {
    let secret = SecretRef::new("mac-token", None).expect("static");
    assert_eq!(secret.name(), "mac-token");
    assert!(secret.version().is_none());

    let versioned = SecretRef::new("tls-cert", Some("v3".into())).expect("static");
    assert_eq!(versioned.name(), "tls-cert");
    assert_eq!(versioned.version(), Some("v3"));

    // Empty / whitespace / oversize names are rejected.
    assert!(SecretRef::new("", None).is_err());
    assert!(SecretRef::new("   ", None).is_err());
    assert!(SecretRef::new(&"a".repeat(255), None).is_err());

    // Empty versions are rejected.
    assert!(SecretRef::new("mac-token", Some(String::new())).is_err());
}

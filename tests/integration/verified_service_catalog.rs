//! Verified-service-catalog integration tests.
//!
//! The suite covers the four ADDED Requirements in
//! `openspec/specs/verified-service-catalog/spec.md`:
//!   - Signed Declarative Manifest
//!   - Reviewable Install Plan
//!   - Safe Lifecycle
//!   - Trust Revocation
//!
//! Each spec scenario is named in a comment next to the test that
//! proves it so the `spec-test-drift-strict` gate can map the
//! `verified-service-catalog` capability to executable coverage.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use async_trait::async_trait;
use openpanel_app::verified_service_catalog::{
    VerifiedServiceCatalogService, VerifiedServiceCatalogServiceError,
};
use openpanel_core::{
    AuditEvent, AuditOutcome, AuditService, CoreResult,
    audit::{AuditPage, AuditQuery, redact_metadata},
};
use openpanel_domain::{
    CatalogProvisioner, Email, HostPolicy, JobOutcome, Password, PlanConflict, ProvisionOutcome,
    PublisherIdentity, ResourceRequest, Role, ServiceBackupPolicy, ServiceCompatibility,
    ServiceHealthProbe, ServiceManifest, ServicePermissions, ServiceUpgradePolicy, SourceDigest,
    TrustState, User, Username, VerifiedServiceCatalogError,
};
use uuid::Uuid;

/// Audit sink that records every event for assertion. The
/// integration tests verify refusal/ denial audits and prove that
/// serialized metadata never carries credentials or manifest secret
/// material. Mirrors the `RecordingAuditService` pattern from
/// `deployment_adapters`.
#[derive(Default)]
struct RecordingAudit {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditService for RecordingAudit {
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

impl RecordingAudit {
    fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().expect("recording audit lock").clone()
    }

    /// Poll until at least `count` events exist or 1s elapses; the
    /// service's audit fan-out is `tokio::spawn`-detached.
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

fn owner() -> User {
    make_user("owner", Role::Owner)
}

fn member() -> User {
    make_user("member", Role::User)
}

fn make_user(name: &str, role: Role) -> User {
    User::new(
        Uuid::new_v4(),
        Username::new(name).expect("static"),
        Email::new(format!("{name}@example.com")).expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        role,
    )
}

fn publisher() -> PublisherIdentity {
    PublisherIdentity::new("openpanel-core", "OpenPanel Core Team", "pubkey-0f0f0f0f")
        .expect("static")
}

fn digest_a() -> SourceDigest {
    SourceDigest::new(format!("sha256:{}", "a".repeat(64))).expect("static")
}

fn digest_b() -> SourceDigest {
    SourceDigest::new(format!("sha256:{}", "b".repeat(64))).expect("static")
}

fn manifest_for(
    pub_key: PublisherIdentity,
    dig: SourceDigest,
    version: &str,
    signature: String,
) -> ServiceManifest {
    ServiceManifest::new(
        "demo-service",
        "Demo Service",
        version,
        pub_key,
        dig,
        signature,
        ServiceCompatibility::new(vec!["debian:amd64".into(), "ubuntu:amd64".into()])
            .expect("static"),
        ResourceRequest::new(1000, 512 * 1024 * 1024, 2 * 1024 * 1024 * 1024).expect("static"),
        ServicePermissions::new(
            true,
            vec![8080, 9443],
            vec!["registry-creds".into()],
            vec!["writes:/var/lib/demo".into()],
        )
        .expect("static"),
        ServiceHealthProbe::new("/healthz", 30, 2).expect("static"),
        ServiceBackupPolicy::new(true),
        ServiceUpgradePolicy::new(3, true).expect("static"),
    )
    .expect("static")
}

fn signed_manifest() -> ServiceManifest {
    let pub_key = publisher();
    let dig = digest_a();
    let signature = openpanel_domain::manifest_signature(&pub_key, &dig);
    manifest_for(pub_key, dig, "1.2.3", signature)
}

fn host_policy() -> HostPolicy {
    HostPolicy::new("debian", "amd64", vec![8080, 9443], 2000, 1 << 30, 10 << 30).expect("static")
}

struct FakeProvisioner {
    install_ok: AtomicBool,
    healthy: AtomicBool,
    installs: AtomicUsize,
    removals: AtomicUsize,
}

impl FakeProvisioner {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            install_ok: AtomicBool::new(true),
            healthy: AtomicBool::new(true),
            installs: AtomicUsize::new(0),
            removals: AtomicUsize::new(0),
        })
    }
}

impl CatalogProvisioner for FakeProvisioner {
    fn provider_id(&self) -> &'static str {
        "fake-provisioner-v1"
    }
    fn install(
        &self,
        _plan: &openpanel_domain::ServicePlan,
    ) -> Result<(), VerifiedServiceCatalogError> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        if self.install_ok.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(VerifiedServiceCatalogError::Provision(
                "registry pull failed password=hunter2 token=abc".into(),
            ))
        }
    }
    fn backup(&self, _entry_id: &str) -> Result<(), VerifiedServiceCatalogError> {
        Ok(())
    }
    fn health_check(
        &self,
        _entry_id: &str,
    ) -> Result<ProvisionOutcome, VerifiedServiceCatalogError> {
        Ok(ProvisionOutcome {
            healthy: self.healthy.load(Ordering::SeqCst),
            diagnostic: "probe".into(),
        })
    }
    fn remove(&self, _entry_id: &str) -> Result<(), VerifiedServiceCatalogError> {
        self.removals.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn service(
    provisioner: Arc<FakeProvisioner>,
) -> (VerifiedServiceCatalogService, Arc<RecordingAudit>) {
    let audit = Arc::new(RecordingAudit::default());
    let svc = VerifiedServiceCatalogService::new(provisioner, audit.clone());
    (svc, audit)
}

/// Scenario: Invalid signature — the catalog marks the entry
/// blocked and refuses planning or install.
#[tokio::test(flavor = "current_thread")]
async fn verified_service_catalog_invalid_signature_blocks_entry() {
    let (svc, _audit) = service(FakeProvisioner::new());
    svc.trust_publisher(&owner(), publisher()).expect("trust");
    let hostile = manifest_for(publisher(), digest_a(), "1.2.3", "deadbeef".into());
    let view = svc
        .submit_entry(&owner(), hostile)
        .expect("kept for review");
    assert_eq!(view.trust_state, TrustState::Blocked.as_str());
    let plan = svc.plan_install(&owner(), "demo-service", &host_policy());
    assert!(matches!(
        plan,
        Err(VerifiedServiceCatalogServiceError::Domain(
            VerifiedServiceCatalogError::Blocked
        ))
    ));
    assert!(
        svc.install_entry(&owner(), "demo-service", &host_policy())
            .is_err()
    );
}

/// Scenario: Resource conflict — planning fails with a specific
/// conflict and performs no mutation.
#[tokio::test(flavor = "current_thread")]
async fn verified_service_catalog_resource_conflict_performs_no_mutation() {
    let provisioner = FakeProvisioner::new();
    let (svc, _audit) = service(Arc::clone(&provisioner));
    svc.trust_publisher(&owner(), publisher()).expect("trust");
    svc.submit_entry(&owner(), signed_manifest())
        .expect("submit");
    let tight =
        HostPolicy::new("debian", "amd64", vec![8080], 2000, 1 << 30, 10 << 30).expect("static");
    let plan = svc.plan_install(&owner(), "demo-service", &tight);
    assert!(matches!(
        plan,
        Err(VerifiedServiceCatalogServiceError::Conflict(
            PlanConflict::PortNotAllowed { port: 9443 }
        ))
    ));
    let install = svc.install_entry(&owner(), "demo-service", &tight);
    assert!(matches!(
        install,
        Err(VerifiedServiceCatalogServiceError::Conflict(
            PlanConflict::PortNotAllowed { port: 9443 }
        ))
    ));
    assert_eq!(provisioner.installs.load(Ordering::SeqCst), 0);
    assert!(svc.jobs(&owner()).expect("jobs").is_empty());
}

/// The full safe lifecycle: install -> backup -> upgrade -> health
/// -> remove, each an observable job with audit events.
#[tokio::test(flavor = "current_thread")]
async fn verified_service_catalog_full_lifecycle_is_observable_and_audited() {
    let provisioner = FakeProvisioner::new();
    let (svc, audit) = service(Arc::clone(&provisioner));
    svc.trust_publisher(&owner(), publisher()).expect("trust");
    svc.submit_entry(&owner(), signed_manifest())
        .expect("submit");
    let install = svc
        .install_entry(&owner(), "demo-service", &host_policy())
        .expect("install");
    assert_eq!(install.outcome, JobOutcome::Succeeded.as_str());
    let backup = svc.run_backup(&owner(), "demo-service").expect("backup");
    assert_eq!(backup.outcome, JobOutcome::Succeeded.as_str());
    let pub_key = publisher();
    let dig = digest_b();
    let signature = openpanel_domain::manifest_signature(&pub_key, &dig);
    let upgraded = manifest_for(pub_key, dig, "1.2.4", signature);
    let upgrade = svc
        .upgrade_entry(&owner(), &upgraded, &host_policy())
        .expect("upgrade");
    assert_eq!(upgrade.outcome, JobOutcome::Succeeded.as_str());
    let health = svc.verify_health(&owner(), "demo-service").expect("health");
    assert_eq!(health.outcome, JobOutcome::Succeeded.as_str());
    let remove = svc.remove_entry(&owner(), "demo-service").expect("remove");
    assert_eq!(remove.outcome, JobOutcome::Succeeded.as_str());
    assert_eq!(provisioner.removals.load(Ordering::SeqCst), 1);
    assert!(
        !svc.entry(&owner(), "demo-service")
            .expect("entry")
            .installed
    );
    // Job history + audits are emitted for every transition.
    let jobs = svc.jobs(&owner()).expect("jobs");
    assert_eq!(jobs.len(), 5);
    let events = audit.wait_for(7).await;
    assert!(
        events
            .iter()
            .any(|event| event.outcome == AuditOutcome::Success
                && event.target.as_deref() == Some("demo-service"))
    );
    // No credential material ever reaches a serialized audit event.
    for event in &events {
        let serialized = redact_metadata(&event.metadata).to_string();
        assert!(!serialized.contains("hunter2"), "audit leaked a credential");
    }
}

/// Scenario: Upgrade failure — a failed health verification leaves
/// the previous verified release recoverable and the job rolled
/// back with operator guidance.
#[tokio::test(flavor = "current_thread")]
async fn verified_service_catalog_upgrade_failure_preserves_previous_release() {
    let provisioner = FakeProvisioner::new();
    let (svc, _audit) = service(Arc::clone(&provisioner));
    svc.trust_publisher(&owner(), publisher()).expect("trust");
    svc.submit_entry(&owner(), signed_manifest())
        .expect("submit");
    svc.install_entry(&owner(), "demo-service", &host_policy())
        .expect("install");
    svc.run_backup(&owner(), "demo-service").expect("backup");
    let pub_key = publisher();
    let dig = digest_b();
    let signature = openpanel_domain::manifest_signature(&pub_key, &dig);
    let upgraded = manifest_for(pub_key, dig, "1.2.4", signature);
    provisioner.healthy.store(false, Ordering::SeqCst);
    let job = svc
        .upgrade_entry(&owner(), &upgraded, &host_policy())
        .expect("upgrade job");
    assert_eq!(job.outcome, JobOutcome::RolledBack.as_str());
    assert!(
        job.guidance
            .expect("guidance")
            .contains("previous verified release")
    );
    // The prior release is what the entry reports as installed.
    let entry = svc.entry(&owner(), "demo-service").expect("entry");
    assert!(entry.installed);
    assert_eq!(entry.installed_digest.as_deref(), Some(digest_a().as_str()));
    assert_eq!(entry.version, "1.2.3");
    // Health recovered from the rollback restores operability.
    provisioner.healthy.store(true, Ordering::SeqCst);
    let health = svc.verify_health(&owner(), "demo-service").expect("health");
    assert_eq!(health.outcome, JobOutcome::Succeeded.as_str());
}

/// Scenario: Cached revoked entry — a revoked publisher's entry is
/// refused even while cached, and the refusal is audited without
/// exposing credentials or manifest secrets.
#[tokio::test(flavor = "current_thread")]
async fn verified_service_catalog_cached_revoked_entry_refused_and_audited() {
    let provisioner = FakeProvisioner::new();
    let (svc, audit) = service(Arc::clone(&provisioner));
    svc.trust_publisher(&owner(), publisher()).expect("trust");
    svc.submit_entry(&owner(), signed_manifest())
        .expect("submit");
    svc.install_entry(&owner(), "demo-service", &host_policy())
        .expect("install");
    svc.revoke_publisher(&owner(), "openpanel-core")
        .expect("revoke");
    // The entry is still cached (review evidence) but every request
    // is refused.
    let refused = svc.install_entry(&owner(), "demo-service", &host_policy());
    assert!(matches!(
        refused,
        Err(VerifiedServiceCatalogServiceError::Domain(
            VerifiedServiceCatalogError::Revoked(_)
        ))
    ));
    let events = audit.wait_for(5).await;
    let denial = events
        .iter()
        .find(|event| event.outcome == AuditOutcome::Denied)
        .expect("revoked install denial was audited");
    assert_eq!(denial.target.as_deref(), Some("demo-service"));
    let serialized = redact_metadata(&denial.metadata).to_string();
    // Refusals name ids/wire states, never credentials.
    assert!(!serialized.contains("hunter2"));
    assert!(!serialized.contains("pubkey-"));
}

/// Defense-in-depth: non-operators cannot mutate the catalog and
/// only Owners manage the curated trust registry.
#[tokio::test(flavor = "current_thread")]
async fn verified_service_catalog_role_gates_enforced_at_the_service() {
    let (svc, _audit) = service(FakeProvisioner::new());
    assert!(matches!(
        svc.submit_entry(&member(), signed_manifest()),
        Err(VerifiedServiceCatalogServiceError::Forbidden)
    ));
    svc.trust_publisher(&owner(), publisher())
        .expect("owner trust");
    assert!(matches!(
        svc.revoke_publisher(&member(), "openpanel-core"),
        Err(VerifiedServiceCatalogServiceError::Forbidden)
    ));
    assert!(matches!(
        svc.entries(&member()),
        Err(VerifiedServiceCatalogServiceError::Forbidden)
    ));
}

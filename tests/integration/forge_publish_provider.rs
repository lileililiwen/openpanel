//! `forge-publish-provider` integration tests.
//!
//! The integration suite covers the four ADDED Requirements in
//! `openspec/changes/forge-publish-provider/specs/forge-publish-provider/spec.md`
//! (live at `openspec/specs/forge-publish-provider/spec.md` after archive):
//!   - Forge provider facade
//!   - Independent lifecycle
//!   - Idempotency and rollback
//!   - Secret and diagnostic redaction
//!
//! Every scenario is named in the spec and matched by fingerprint
//! here so the `scripts/check-spec-test-drift.sh` gate can verify
//! scenario coverage.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_app::{
    ApplicationDeliveryService, DeploymentAdapterService, ForgePublishProvider,
    ForgePublishProviderConfig,
};
use openpanel_core::audit::NoopAuditService;
use openpanel_domain::forge_publish_provider::{
    FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION, ForgeProviderOperation, ForgeProviderStatus,
    parse_request,
};
use openpanel_domain::{
    AdapterManifest, ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind,
    BuildAdapter, DeploymentAction, DeploymentAdapter, DeploymentEvidence, DeploymentPlan,
    DeploymentState, Email, Environment, EnvironmentKind, HealthMethod, Password,
    RUNTIME_CONTRACT_VERSION, Release, ReleaseDigest, ReleaseOutcome, ReleaseState, Role,
    RuntimeAdapter, SecretSetRef, User, Username,
};
use uuid::Uuid;

const SOURCE_REF: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b";
const RELEASE_DIGEST: &str =
    "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("owner").expect("static"),
        Email::new("owner@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Owner,
    )
}

struct FixedBuildAdapter;

impl BuildAdapter for FixedBuildAdapter {
    fn adapter_id(&self) -> &'static str {
        "fake-build-v1"
    }

    fn resolve_digest(
        &self,
        _application_id: Uuid,
        _source: &ApplicationSource,
    ) -> Result<ReleaseDigest, ApplicationDeliveryError> {
        ReleaseDigest::new(RELEASE_DIGEST, Utc::now())
    }
}

struct HealthyRuntimeAdapter;

impl RuntimeAdapter for HealthyRuntimeAdapter {
    fn adapter_id(&self) -> &'static str {
        "fake-runtime-v1"
    }

    fn deploy(
        &self,
        _release: &Release,
        _environment: &Environment,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: ReleaseState::Healthy,
            diagnostic: "healthy".into(),
        })
    }

    fn rollback(
        &self,
        _release: &Release,
        _environment: &Environment,
        _target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: ReleaseState::RolledBack,
            diagnostic: "rolled back".into(),
        })
    }
}

struct FailingRuntimeAdapter;

impl RuntimeAdapter for FailingRuntimeAdapter {
    fn adapter_id(&self) -> &'static str {
        "fake-runtime-v1"
    }

    fn deploy(
        &self,
        _release: &Release,
        _environment: &Environment,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: ReleaseState::Failed,
            diagnostic: "health check failed".into(),
        })
    }

    fn rollback(
        &self,
        _release: &Release,
        _environment: &Environment,
        _target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: ReleaseState::RolledBack,
            diagnostic: "rolled back".into(),
        })
    }
}

struct StubAdapter {
    manifest: AdapterManifest,
    state: DeploymentState,
    diagnostic: String,
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
        DeploymentEvidence::new(
            plan.target(),
            plan.adapter(),
            plan.action(),
            plan.operation_key().cloned(),
            plan.release_digest(),
            self.state,
            started_at,
            started_at + Duration::seconds(1),
            self.diagnostic.clone(),
        )
        .expect("valid evidence")
    }
}

fn default_manifest() -> AdapterManifest {
    AdapterManifest::new(
        "openpanel-default",
        "Forge Default Adapter",
        RUNTIME_CONTRACT_VERSION,
        "forge://",
        vec![
            DeploymentAction::Preflight,
            DeploymentAction::Deploy,
            DeploymentAction::Verify,
            DeploymentAction::Rollback,
        ],
        true,
        HealthMethod::HttpGet {
            url: "http://127.0.0.1:8080/healthz".into(),
            expect_body_contains: Some("ok".into()),
        },
        vec!["prod".into()],
    )
    .expect("valid manifest")
}

struct Fixture {
    provider: ForgePublishProvider,
    delivery: Arc<ApplicationDeliveryService>,
    owner: User,
    app_id: Uuid,
}

impl Fixture {
    fn new(
        adapter_state: DeploymentState,
        adapter_diagnostic: &str,
        runtime: Arc<dyn RuntimeAdapter>,
    ) -> Self {
        let owner = owner();
        let audit = Arc::new(NoopAuditService);
        let delivery = Arc::new(ApplicationDeliveryService::new(
            Arc::new(FixedBuildAdapter),
            runtime,
            audit.clone(),
        ));
        let adapter: Arc<dyn DeploymentAdapter> = Arc::new(StubAdapter {
            manifest: default_manifest(),
            state: adapter_state,
            diagnostic: adapter_diagnostic.to_string(),
        });
        let adapters = Arc::new(DeploymentAdapterService::new(vec![adapter], audit.clone()));
        let provider = ForgePublishProvider::new(
            ForgePublishProviderConfig::default(),
            delivery.clone(),
            adapters,
            audit,
        );
        let source = ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/acme/web.git",
            SOURCE_REF,
        )
        .expect("valid source");
        let application = delivery
            .register_application(&owner, "web", source, "make build")
            .expect("register application");
        delivery
            .add_environment(
                &owner,
                application.id(),
                EnvironmentKind::Production,
                "production",
                10,
                SecretSetRef::new("prod-secrets", None).expect("valid secret set"),
                None,
            )
            .expect("add environment");
        Self {
            provider,
            delivery,
            owner,
            app_id: application.id(),
        }
    }

    fn healthy(adapter_state: DeploymentState, diagnostic: &str) -> Self {
        Self::new(adapter_state, diagnostic, Arc::new(HealthyRuntimeAdapter))
    }
}

fn request_for(
    operation: &str,
    operation_id: Option<Uuid>,
    environment: &str,
) -> openpanel_domain::forge_publish_provider::ForgeProviderRequest {
    let revision = serde_json::json!({
        "kind": "git",
        "url": "https://example.com/acme/web.git",
        "source_ref": SOURCE_REF,
        "release_digest": RELEASE_DIGEST,
        "environment": environment,
    });
    let mut params = serde_json::json!({
        "protocol": FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION,
        "operation": operation,
        "project": { "slug": "acme/web", "label": null },
        "revision": revision,
        "operation_id": null,
        "secret_set": { "secret_set": "prod", "version": null },
        "dry_run": false,
    });
    if let Some(id) = operation_id {
        params["operation_id"] = serde_json::json!({ "operation_id": id, "dedupe_key": null });
    }
    if operation == "capabilities" {
        params["revision"] = serde_json::Value::Null;
        params["secret_set"] = serde_json::Value::Null;
    }
    parse_request(&params).expect("valid request")
}

#[tokio::test]
async fn forge_publish_provider_facade_capabilities_preflight_publish_verify() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "deployed");
    let capabilities = fixture
        .provider
        .handle_request(
            request_for("capabilities", None, "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(capabilities.status, ForgeProviderStatus::Succeeded);
    assert_eq!(capabilities.provider.operations.len(), 5);

    let preflight = fixture
        .provider
        .handle_request(
            request_for("preflight", Some(Uuid::new_v4()), "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(preflight.status, ForgeProviderStatus::DryRun);

    let publish = fixture
        .provider
        .handle_request(
            request_for("publish", Some(Uuid::new_v4()), "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(publish.status, ForgeProviderStatus::Succeeded);
    assert!(publish.terminal.is_some());

    let verify = fixture
        .provider
        .handle_request(
            request_for("verify", Some(Uuid::new_v4()), "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(verify.status, ForgeProviderStatus::Succeeded);
    assert_eq!(verify.health.expect("health").health, "healthy");
}

#[tokio::test]
async fn forge_publish_provider_duplicate_operation_returns_existing_evidence() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "deployed");
    let operation_id = Uuid::new_v4();
    let first = fixture
        .provider
        .handle_request(
            request_for("publish", Some(operation_id), "production"),
            &fixture.owner,
        )
        .await;
    let second = fixture
        .provider
        .handle_request(
            request_for("publish", Some(operation_id), "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(first.terminal, second.terminal);
    assert_eq!(fixture.provider.provider_stats().replays, 1);
}

#[tokio::test]
async fn forge_publish_provider_health_failure_is_reported() {
    let fixture = Fixture::healthy(DeploymentState::Failed, "health check failed");
    let response = fixture
        .provider
        .handle_request(
            request_for("publish", Some(Uuid::new_v4()), "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Unhealthy);
}

#[tokio::test]
async fn forge_publish_provider_rollback_restores_a_prior_release() {
    let fixture = Fixture::new(
        DeploymentState::Ready,
        "ok",
        Arc::new(FailingRuntimeAdapter),
    );
    let release = fixture
        .delivery
        .build_release(&fixture.owner, fixture.app_id, "production")
        .expect("build release");
    fixture
        .delivery
        .promote(
            &fixture.owner,
            fixture.app_id,
            "production",
            release.id(),
            "PROMOTE",
        )
        .expect("promote");

    let response = fixture
        .provider
        .handle_request(
            request_for("rollback", Some(Uuid::new_v4()), "production"),
            &fixture.owner,
        )
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    let rollback = response.rollback.expect("rollback payload");
    assert_eq!(rollback.outcome, "succeeded");
    assert_eq!(rollback.restored_release_id, Some(release.id()));
}

#[tokio::test]
async fn forge_publish_provider_independent_lifecycle_disabled_by_default() {
    let handle = openpanel_app::ForgeProviderProcessHandle::default();
    assert!(handle.provider_status().main_service_operational());
    let mut disabled = openpanel_app::ForgeProviderProcessHandle::new(false);
    assert_eq!(
        disabled.start_provider(),
        openpanel_app::ForgeProviderProcessStatus::Disabled
    );
}

#[tokio::test]
async fn forge_publish_provider_redacts_secret_diagnostics() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "ran password=hunter2 deploy");
    let response = fixture
        .provider
        .handle_request(
            request_for("verify", Some(Uuid::new_v4()), "production"),
            &fixture.owner,
        )
        .await;
    let diagnostic = response.diagnostic.expect("diagnostic");
    assert_eq!(diagnostic, "[REDACTED]");
    assert!(!diagnostic.contains("hunter2"));
}

#[tokio::test]
async fn forge_publish_provider_decodes_a_jsonrpc_frame() {
    let request = request_for("verify", Some(Uuid::new_v4()), "production");
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": openpanel_app::FORGE_JSONRPC_METHOD,
        "params": serde_json::to_value(&request).expect("serialise"),
    })
    .to_string();
    let (decoded, _) = openpanel_app::decode_request(&frame).expect("decode");
    assert_eq!(decoded.operation, ForgeProviderOperation::Verify);
}

/// Path to the on-disk Forge provider conformance fixtures.
fn conformance_frame(operation: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/forge-publish-provider")
        .join(format!("{operation}.json"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[tokio::test]
async fn forge_publish_provider_conformance_fixtures_round_trip() {
    for operation in ["capabilities", "preflight", "publish", "verify", "rollback"] {
        let frame = conformance_frame(operation);
        let (decoded, id) = openpanel_app::decode_request(&frame).expect("decode fixture");
        assert_eq!(decoded.operation.as_str(), operation);
        assert!(id.is_some());
    }

    let fixture = Fixture::healthy(DeploymentState::Ready, "conformance ok");
    let (capabilities, _) =
        openpanel_app::decode_request(&conformance_frame("capabilities")).expect("capabilities");
    let response = fixture
        .provider
        .handle_request(capabilities, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    assert_eq!(response.provider.operations.len(), 5);

    let (preflight, _) =
        openpanel_app::decode_request(&conformance_frame("preflight")).expect("preflight");
    let response = fixture
        .provider
        .handle_request(preflight, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::DryRun);

    let (verify, _) = openpanel_app::decode_request(&conformance_frame("verify")).expect("verify");
    let response = fixture
        .provider
        .handle_request(verify, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    assert_eq!(response.health.expect("health").health, "healthy");
}

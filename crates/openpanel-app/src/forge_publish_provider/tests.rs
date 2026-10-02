//! Forge publish provider contract tests. These exercise the
//! facade against the real application-delivery and
//! deployment-adapter services with test doubles for the typed
//! adapter ports, plus the JSON-RPC codec and the process
//! lifecycle harness.

use std::sync::Arc;

use chrono::{Duration, Utc};
use uuid::Uuid;

use openpanel_core::audit::NoopAuditService;
use openpanel_domain::forge_publish_provider::{
    FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION, ForgeProviderOperation, ForgeProviderRequest,
    ForgeProviderStatus, parse_request,
};
use openpanel_domain::{
    AdapterManifest, ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind,
    BuildAdapter, DeploymentAction, DeploymentAdapter, DeploymentEvidence, DeploymentState, Email,
    Environment, EnvironmentKind, HealthMethod, Password, RUNTIME_CONTRACT_VERSION, Release,
    ReleaseDigest, ReleaseOutcome, ReleaseState, Role, RuntimeAdapter, SecretSetRef, User,
    Username,
};

use super::facade::{ForgePublishProvider, ForgePublishProviderConfig};
use super::lifecycle::{
    ForgeProviderProcessHandle, ForgeProviderProcessStatus, parse_provider_enabled,
};
use super::protocol::{FORGE_JSONRPC_METHOD, JsonRpcParseError, decode_request, encode_response};

use crate::application_delivery::ApplicationDeliveryService;
use crate::deployment_adapters::DeploymentAdapterService;

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

fn member() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("member").expect("static"),
        Email::new("member@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::User,
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
        _build_command: &str,
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
        plan: &openpanel_domain::DeploymentPlan,
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

fn default_manifest(secret_refs: Vec<String>) -> AdapterManifest {
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
        secret_refs,
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
            manifest: default_manifest(vec!["prod".into()]),
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

    fn releases(&self) -> Vec<Release> {
        self.delivery
            .list_releases(&self.owner, self.app_id)
            .expect("list releases")
    }
}

fn request_for(
    operation: &str,
    operation_id: Option<Uuid>,
    environment: &str,
) -> ForgeProviderRequest {
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
async fn capabilities_advertise_every_operation() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "ok");
    let request = request_for("capabilities", None, "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    let capabilities = response.capabilities.expect("capabilities payload");
    assert_eq!(
        capabilities.provider.provider_id,
        "openpanel-forge-publish-provider"
    );
    assert_eq!(capabilities.provider.operations.len(), 5);
}

#[tokio::test]
async fn preflight_is_a_dry_run_without_promoting() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "dry-run ok");
    let request = request_for("preflight", Some(Uuid::new_v4()), "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::DryRun);
    assert!(fixture.releases().is_empty());
}

#[tokio::test]
async fn publish_returns_terminal_evidence_and_promotes() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "deployed");
    let request = request_for("publish", Some(Uuid::new_v4()), "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    let terminal = response.terminal.expect("terminal evidence");
    assert_eq!(terminal.release_digest, RELEASE_DIGEST);
    assert_eq!(
        response.health.expect("health").health,
        "healthy".to_string()
    );
    let releases = fixture.releases();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].state(), ReleaseState::Healthy);
}

#[tokio::test]
async fn verify_reports_the_current_health() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "healthy");
    let request = request_for("verify", Some(Uuid::new_v4()), "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    assert_eq!(response.health.expect("health").health, "healthy");
}

#[tokio::test]
async fn rollback_uses_the_application_delivery_policy() {
    let fixture = Fixture::new(
        DeploymentState::Ready,
        "ok",
        Arc::new(FailingRuntimeAdapter),
    );
    let release = fixture
        .delivery
        .build_release(&fixture.owner, fixture.app_id, "production")
        .expect("build release");
    let failed = fixture
        .delivery
        .promote(
            &fixture.owner,
            fixture.app_id,
            "production",
            release.id(),
            "PROMOTE",
        )
        .expect("promote");
    assert_eq!(failed.state(), ReleaseState::Failed);

    let request = request_for("rollback", Some(Uuid::new_v4()), "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Succeeded);
    let rollback = response.rollback.expect("rollback payload");
    assert_eq!(rollback.outcome, "succeeded");
    assert_eq!(rollback.restored_release_id, Some(release.id()));
}

#[tokio::test]
async fn duplicate_operation_id_replays_terminal_evidence() {
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
    assert_eq!(fixture.releases().len(), 1);
}

#[tokio::test]
async fn duplicate_operation_id_against_another_environment_conflicts() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "deployed");
    let operation_id = Uuid::new_v4();
    fixture
        .provider
        .handle_request(
            request_for("publish", Some(operation_id), "production"),
            &fixture.owner,
        )
        .await;
    let second = fixture
        .provider
        .handle_request(
            request_for("publish", Some(operation_id), "staging"),
            &fixture.owner,
        )
        .await;
    assert_eq!(second.status, ForgeProviderStatus::Conflict);
}

#[tokio::test]
async fn publish_with_a_failing_adapter_is_unhealthy() {
    let fixture = Fixture::healthy(DeploymentState::Failed, "health check failed");
    let request = request_for("publish", Some(Uuid::new_v4()), "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::Unhealthy);
    assert!(
        fixture
            .releases()
            .iter()
            .all(|release| release.state() != ReleaseState::Healthy)
    );
}

#[tokio::test]
async fn secret_bearing_diagnostics_are_redacted() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "ran password=hunter2 deploy");
    let request = request_for("verify", Some(Uuid::new_v4()), "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    let diagnostic = response.diagnostic.expect("diagnostic");
    assert_eq!(diagnostic, "[REDACTED]");
    assert!(!diagnostic.contains("hunter2"));
}

#[tokio::test]
async fn non_operator_callers_are_denied() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "ok");
    let caller = member();
    let request = request_for("publish", Some(Uuid::new_v4()), "production");
    let response = fixture.provider.handle_request(request, &caller).await;
    assert_eq!(response.status, ForgeProviderStatus::Failed);
    assert!(fixture.releases().is_empty());
}

#[tokio::test]
async fn missing_revision_is_an_invalid_request() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "ok");
    let mut request = request_for("publish", Some(Uuid::new_v4()), "production");
    request.revision = None;
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    assert_eq!(response.status, ForgeProviderStatus::InvalidRequest);
    assert!(fixture.releases().is_empty());
}

#[tokio::test]
async fn protocol_encodes_terminal_responses() {
    let fixture = Fixture::healthy(DeploymentState::Ready, "ok");
    let request = request_for("capabilities", None, "production");
    let response = fixture
        .provider
        .handle_request(request, &fixture.owner)
        .await;
    let frame = encode_response(&response, Some(serde_json::json!(7))).expect("encode");
    let value: serde_json::Value = serde_json::from_str(&frame).expect("json");
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 7);
    assert_eq!(value["result"]["status"], "succeeded");
}

#[test]
fn lifecycle_disabled_provider_never_starts() {
    let mut handle = ForgeProviderProcessHandle::new(false);
    assert_eq!(
        handle.provider_status(),
        ForgeProviderProcessStatus::Disabled
    );
    assert_eq!(
        handle.start_provider(),
        ForgeProviderProcessStatus::Disabled
    );
    assert!(handle.provider_status().main_service_operational());
    assert_eq!(handle.stop_provider(), ForgeProviderProcessStatus::Disabled);
}

#[test]
fn lifecycle_enabled_provider_starts_and_stops() {
    let mut handle = ForgeProviderProcessHandle::new(true);
    assert_eq!(handle.start_provider(), ForgeProviderProcessStatus::Running);
    assert_eq!(handle.stop_provider(), ForgeProviderProcessStatus::Stopped);
    assert!(handle.provider_status().main_service_operational());
}

#[test]
fn lifecycle_failure_keeps_the_main_service_operational() {
    let mut handle = ForgeProviderProcessHandle::new(true);
    handle.start_provider();
    assert_eq!(
        handle.mark_provider_failed(),
        ForgeProviderProcessStatus::Failed
    );
    assert!(handle.provider_status().main_service_operational());
}

#[test]
fn lifecycle_enable_flag_is_strict() {
    assert!(parse_provider_enabled("true"));
    assert!(parse_provider_enabled("ON"));
    assert!(parse_provider_enabled(" 1 "));
    assert!(!parse_provider_enabled("0"));
    assert!(!parse_provider_enabled(""));
    assert!(!parse_provider_enabled("maybe"));
}

#[test]
fn protocol_decodes_a_publish_frame() {
    let request = request_for("publish", Some(Uuid::new_v4()), "production");
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": FORGE_JSONRPC_METHOD,
        "params": serde_json::to_value(&request).expect("serialise"),
    })
    .to_string();
    let (decoded, id) = decode_request(&frame).expect("decode");
    assert_eq!(decoded.operation, ForgeProviderOperation::Publish);
    assert_eq!(id, Some(serde_json::json!(7)));
}

#[test]
fn protocol_rejects_an_unknown_method() {
    let request = request_for("verify", Some(Uuid::new_v4()), "production");
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "forge.other",
        "params": serde_json::to_value(&request).expect("serialise"),
    })
    .to_string();
    let err = decode_request(&frame).expect_err("unknown method");
    assert!(matches!(err, JsonRpcParseError::Method(_)));
}

#[test]
fn protocol_rejects_a_bad_jsonrpc_version() {
    let request = request_for("verify", Some(Uuid::new_v4()), "production");
    let frame = serde_json::json!({
        "jsonrpc": "1.0",
        "id": 1,
        "method": FORGE_JSONRPC_METHOD,
        "params": serde_json::to_value(&request).expect("serialise"),
    })
    .to_string();
    let err = decode_request(&frame).expect_err("bad version");
    assert!(matches!(err, JsonRpcParseError::Frame(_)));
}

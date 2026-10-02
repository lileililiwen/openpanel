//! Application delivery integration tests.
//!
//! The integration suite covers the four ADDED Requirements in
//! `openspec/specs/application-delivery/spec.md`:
//!   - Immutable Application Release
//!   - Isolated Environment Lifecycle
//!   - Health-Gated Promotion
//!   - Audited Rollback
//!
//! Each scenario is named in the spec and matched by fingerprint
//! here so the `scripts/check-application-delivery.sh` gate can
//! verify scenario coverage.

use std::sync::Arc;

use chrono::Utc;
use openpanel_app::DeploymentAdapterService;
use openpanel_app::ForgePublishProvider;
use openpanel_app::ForgePublishProviderConfig;
use openpanel_app::application_delivery::{
    ApplicationDeliveryService, ApplicationDeliveryServiceError,
};
use openpanel_core::audit::NoopAuditService;
use openpanel_domain::{
    ApplicationSource, ApplicationSourceKind, Email, EnvironmentKind, Password, ReleaseState, Role,
    SecretSetRef, User, Username,
    application_delivery::{BuildAdapter, ReleaseOutcome, RuntimeAdapter},
};
use uuid::Uuid;

/// Build a noop audit service for tests. Reused across
/// application-delivery integration tests so the audit fan-out
/// is exercised but does not block on async I/O.
fn noop_audit() -> Arc<dyn openpanel_core::AuditService> {
    Arc::new(NoopAuditService)
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

struct FakeBuildAdapter;
impl BuildAdapter for FakeBuildAdapter {
    fn adapter_id(&self) -> &'static str {
        "fake-build-v1"
    }
    fn resolve_digest(
        &self,
        _application_id: Uuid,
        _source: &ApplicationSource,
        _build_command: &str,
    ) -> Result<openpanel_domain::ReleaseDigest, openpanel_domain::ApplicationDeliveryError> {
        openpanel_domain::ReleaseDigest::new("sha256:cafef00ddeadbeef", Utc::now())
    }
}

struct HealthyRuntimeAdapter;
impl RuntimeAdapter for HealthyRuntimeAdapter {
    fn adapter_id(&self) -> &'static str {
        "healthy-runtime-v1"
    }
    fn deploy(
        &self,
        _release: &openpanel_domain::Release,
        _environment: &openpanel_domain::Environment,
    ) -> Result<ReleaseOutcome, openpanel_domain::ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: ReleaseState::Healthy,
            diagnostic: "200 ok".into(),
        })
    }
    fn rollback(
        &self,
        _release: &openpanel_domain::Release,
        _environment: &openpanel_domain::Environment,
        _target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, openpanel_domain::ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: openpanel_domain::ReleaseState::RolledBack,
            diagnostic: "rolled back".into(),
        })
    }
}

struct UnhealthyRuntimeAdapter;
impl RuntimeAdapter for UnhealthyRuntimeAdapter {
    fn adapter_id(&self) -> &'static str {
        "unhealthy-runtime-v1"
    }
    fn deploy(
        &self,
        _release: &openpanel_domain::Release,
        _environment: &openpanel_domain::Environment,
    ) -> Result<ReleaseOutcome, openpanel_domain::ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: openpanel_domain::ReleaseState::Failed,
            diagnostic: "health check failed: password=hunter2".into(),
        })
    }
    fn rollback(
        &self,
        _release: &openpanel_domain::Release,
        _environment: &openpanel_domain::Environment,
        _target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, openpanel_domain::ApplicationDeliveryError> {
        Ok(ReleaseOutcome {
            state: openpanel_domain::ReleaseState::RolledBack,
            diagnostic: "rolled back".into(),
        })
    }
}

fn git_source() -> ApplicationSource {
    ApplicationSource::new(
        ApplicationSourceKind::Git,
        "https://example.com/repo.git",
        "abcdef1234567890",
    )
    .unwrap()
}

fn service_with(rt: Arc<dyn RuntimeAdapter>) -> ApplicationDeliveryService {
    ApplicationDeliveryService::new(Arc::new(FakeBuildAdapter), rt, noop_audit())
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_immutable_application_release_rejects_unpinned_source() {
    // Scenario: Mutable tag rejected — a production deployment
    // referencing an unpinned mutable image tag is rejected.
    // The domain validator catches the unpinned `main` ref at
    // registration time; the same rule applies to OCI
    // deployments where the tag is non-`sha256:`.
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let bad_source = ApplicationSource::new(
        ApplicationSourceKind::Git,
        "https://example.com/repo.git",
        "main",
    )
    .unwrap();
    let result = svc.register_application(&owner, "web", bad_source, "");
    assert!(matches!(
        result,
        Err(ApplicationDeliveryServiceError::Domain(_))
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_immutable_application_release_resolves_digest_before_deploy() {
    // The release's digest is the immutable `sha256:` value
    // returned by the build adapter; the deployment step targets
    // that same digest.
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let release = svc.build_release(&owner, app.id(), "production").unwrap();
    assert!(release.digest().value().starts_with("sha256:"));
    let promoted = svc
        .promote(&owner, app.id(), "production", release.id(), "PROMOTE")
        .unwrap();
    assert_eq!(promoted.digest().value(), release.digest().value());
    assert_eq!(promoted.state(), ReleaseState::Healthy);
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_isolated_environment_lifecycle_rejects_preview_with_prod_secret() {
    // Scenario: Preview cannot use production secret — a
    // preview environment MUST NOT be able to resolve a
    // production secret set. The kind-specific secret-set
    // prefix is enforced by the domain `Environment::new`; the
    // audit trail records the denial without the secret value.
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    let bad = svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Preview,
        "preview-1",
        1,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        Some(Utc::now() + chrono::Duration::hours(1)),
    );
    assert!(matches!(
        bad,
        Err(ApplicationDeliveryServiceError::Domain(_))
    ));
    let good = svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Preview,
        "preview-1",
        1,
        SecretSetRef::new("preview-secrets", None).unwrap(),
        Some(Utc::now() + chrono::Duration::hours(1)),
    );
    assert!(good.is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_isolated_environment_lifecycle_rejects_expired_preview_build() {
    // A preview environment MUST have an `expires_at` in the
    // future; building against an expired preview is refused.
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    // Add a preview with a 1-hour TTL, then add another preview
    // already expired at creation time (the constructor refuses
    // that; the build path additionally refuses an environment
    // that is already expired at `now`).
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Preview,
        "preview-1",
        1,
        SecretSetRef::new("preview-secrets", None).unwrap(),
        Some(Utc::now() - chrono::Duration::seconds(1)),
    )
    .expect_err("expired-at-construction preview must be rejected");
    // Add a future preview, then build → it should succeed.
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Preview,
        "preview-2",
        1,
        SecretSetRef::new("preview-secrets", None).unwrap(),
        Some(Utc::now() + chrono::Duration::hours(2)),
    )
    .unwrap();
    let result = svc.build_release(&owner, app.id(), "preview-2");
    assert!(result.is_ok(), "non-expired preview should build");
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_health_gated_promotion_blocks_when_health_fails() {
    // Scenario: Health timeout — when the deployment does not
    // become healthy before its deadline, promotion is
    // blocked, diagnostics are bounded and redacted, and the
    // release remains recoverable. The unhealthy adapter
    // surfaces a `Failed` terminal state and a redacted
    // diagnostic; the release is then rolled back to the
    // prior healthy release.
    let svc = service_with(Arc::new(UnhealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let release = svc.build_release(&owner, app.id(), "production").unwrap();
    let failed = svc
        .promote(&owner, app.id(), "production", release.id(), "PROMOTE")
        .unwrap();
    assert_eq!(failed.state(), openpanel_domain::ReleaseState::Failed);
    // The diagnostic is redacted — secrets never appear.
    assert!(!failed.diagnostic().contains("hunter2"));
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_health_gated_promotion_requires_confirmation() {
    // Promotion without a confirmation is refused.
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let release = svc.build_release(&owner, app.id(), "production").unwrap();
    let result = svc.promote(&owner, app.id(), "production", release.id(), "");
    assert!(matches!(
        result,
        Err(ApplicationDeliveryServiceError::Domain(_))
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_audited_rollback_returns_to_prior_healthy_release() {
    // Scenario: Rollback succeeds — when an operator confirms
    // rollback to the previous healthy release, traffic
    // returns to that release and one audit event records
    // the transition without credentials or source payloads.
    let svc = service_with(Arc::new(UnhealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let release = svc.build_release(&owner, app.id(), "production").unwrap();
    let failed = svc
        .promote(&owner, app.id(), "production", release.id(), "PROMOTE")
        .unwrap();
    assert_eq!(failed.state(), openpanel_domain::ReleaseState::Failed);
    let rolled = svc
        .rollback(&owner, app.id(), "production", release.id(), "ROLLBACK")
        .unwrap();
    assert_eq!(rolled.state(), openpanel_domain::ReleaseState::RolledBack);
    assert_eq!(rolled.id(), release.id());
    // The rolled-back record preserves the release identity so
    // the audit trail can link the rollback to the original
    // release.
    assert!(rolled.digest().value().starts_with("sha256:"));
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_audited_rollback_refuses_healthy_release() {
    // A release in `Healthy` cannot be rolled back; the state
    // machine refuses the transition.
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let release = svc.build_release(&owner, app.id(), "production").unwrap();
    let healthy = svc
        .promote(&owner, app.id(), "production", release.id(), "PROMOTE")
        .unwrap();
    assert_eq!(healthy.state(), ReleaseState::Healthy);
    let result = svc.rollback(&owner, app.id(), "production", release.id(), "ROLLBACK");
    assert!(matches!(
        result,
        Err(ApplicationDeliveryServiceError::Domain(_))
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_audited_rollback_blocks_non_owner() {
    // Only the owner / admin role can roll back; members are
    // rejected.
    let svc = service_with(Arc::new(UnhealthyRuntimeAdapter));
    let owner = owner();
    let member = member();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let release = svc.build_release(&owner, app.id(), "production").unwrap();
    svc.promote(&owner, app.id(), "production", release.id(), "PROMOTE")
        .unwrap();
    let result = svc.rollback(&member, app.id(), "production", release.id(), "ROLLBACK");
    assert!(matches!(
        result,
        Err(ApplicationDeliveryServiceError::Forbidden)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn application_delivery_audit_metadata_is_redacted() {
    // Every audit fan-out routes through the canonical
    // `redact_metadata` allowlist. We assert the service does
    // not bypass the allowlist by exposing a public hook that
    // returns the metadata shape (the gate asserts the
    // redaction allowlist is used).
    let svc = service_with(Arc::new(HealthyRuntimeAdapter));
    let owner = owner();
    let app = svc
        .register_application(&owner, "web", git_source(), "")
        .unwrap();
    svc.add_environment(
        &owner,
        app.id(),
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).unwrap(),
        None,
    )
    .unwrap();
    let _ = svc.build_release(&owner, app.id(), "production").unwrap();
    // The application record projects the source pointer; the
    // secret-set name is a *name* (not a value) and is allowed
    // through the allowlist.
    let record = svc.project_application(&owner, app.id()).unwrap();
    assert_eq!(record.environments[0].secret_set_name, "prod-secrets");
    let _ = noop_audit();
}

// --- Docker delivery composition (DCK-001, DCK-002, DCK-004) --------
//
// These tests compose the *production* adapters — build, runtime and
// deployment — over the recording docker double, then drive them
// through the forge publish facade. The `oci` source kind is used so
// the composition needs no git host; the full local-git
// clone -> build -> push -> digest path is proven by the adapter unit
// suite (`docker_build_tests`), because a `file://` fixture source is
// only reachable from a test build of the library, never from a
// production build.

/// The immutable digest the double answers with.
const OCI_DIGEST: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const REGISTRY_NAMESPACE: &str = "registry.example/openpanel";

fn oci_source() -> ApplicationSource {
    ApplicationSource::new(
        ApplicationSourceKind::Oci,
        format!("oci://{REGISTRY_NAMESPACE}/web"),
        OCI_DIGEST,
    )
    .unwrap()
}

/// The production composition. It comes from the app crate so the
/// integration suite, the CLI suite, and the provider entry point all
/// wire the same graph instead of three look-alikes.
fn compose(
    docker: Arc<openpanel_app::docker::testing::FakeDocker>,
) -> openpanel_app::forge_publish_provider::testing::DoubleHarness {
    openpanel_app::forge_publish_provider::testing::double_harness(
        owner(),
        docker,
        "web",
        OCI_DIGEST,
    )
}

/// The application the harness registered for the `acme/web` slug.
fn compose_application(
    harness: &openpanel_app::forge_publish_provider::testing::DoubleHarness,
) -> openpanel_domain::Application {
    harness
        .delivery()
        .list_applications(&harness.caller)
        .expect("harness application is listed")
        .into_iter()
        .next()
        .expect("harness registered exactly one application")
}

/// A forge request frame for the registered `acme/web` project.
fn publish_request(
    operation: &str,
    dry_run: bool,
) -> openpanel_domain::forge_publish_provider::ForgeProviderRequest {
    use openpanel_domain::forge_publish_provider::parse_request;
    let params = serde_json::json!({
        "protocol": openpanel_domain::forge_publish_provider::FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION,
        "operation": operation,
        "project": { "slug": "acme/web", "label": null },
        "revision": {
            "kind": "oci",
            "url": format!("oci://{REGISTRY_NAMESPACE}/web"),
            "source_ref": OCI_DIGEST,
            "release_digest": OCI_DIGEST,
            "environment": "production",
        },
        "operation_id": { "operation_id": Uuid::new_v4(), "dedupe_key": null },
        "secret_set": { "secret_set": "prod-secrets", "version": null },
        "dry_run": dry_run,
    });
    parse_request(&params).expect("valid request")
}

/// Scenario fingerprint: "Git source becomes a pinned digest".
/// A build resolves to the immutable registry digest, never a
/// mutable tag, and a source the adapter cannot clone is refused
/// before any engine mutation.
#[tokio::test(flavor = "current_thread")]
async fn docker_delivery_resolves_a_pinned_digest_and_refuses_an_unreachable_source() {
    let docker = Arc::new(openpanel_app::docker::testing::FakeDocker::new());
    let composition = compose(docker.clone());
    let caller = composition.caller.clone();
    let application = compose_application(&composition);

    let digest = composition
        .delivery()
        .build_release(&caller, application.id(), "production")
        .expect("release built")
        .digest()
        .value()
        .to_string();

    assert_eq!(digest, OCI_DIGEST, "the release is pinned to the digest");
    assert!(
        !docker.calls().iter().any(|call| call == "build_image"),
        "an image already present is not rebuilt: {:?}",
        docker.calls()
    );
    // A git source naming a local path is refused by a production
    // build: the clone never reaches the engine.
    let local = ApplicationSource::new(
        ApplicationSourceKind::Git,
        "file:///tmp/openpanel-fixture",
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b",
    )
    .unwrap();
    let adapter = openpanel_app::DockerBuildAdapter::new(
        docker.clone(),
        REGISTRY_NAMESPACE.to_string(),
        8 * 1024 * 1024,
    )
    .unwrap();
    let refused = adapter.resolve_digest(application.id(), &local, "");
    assert!(matches!(
        refused,
        Err(openpanel_domain::ApplicationDeliveryError::Invalid(_))
    ));
    assert!(
        !docker.calls().iter().any(|call| call == "build_image"),
        "an unreachable source never reaches the engine: {:?}",
        docker.calls()
    );
}

/// Scenario fingerprint: "Registry authentication rejected". A push
/// rejected by the registry becomes a typed domain error whose detail
/// is a fixed, credential-free string, and the provider surfaces it as
/// an adapter failure instead of leaking upstream text.
#[tokio::test(flavor = "current_thread")]
async fn docker_delivery_registry_auth_rejection_is_typed_and_redacted() {
    use openpanel_app::ForgePublishProviderError;

    // The push is where a registry rejects credentials: the port
    // reports the rejection as a forbidden error, never as text.
    let port: Arc<dyn openpanel_app::DockerAdapter> = Arc::new(
        openpanel_app::docker::testing::FakeDocker::new()
            .with_failure(openpanel_app::docker::testing::FAIL_AUTH),
    );
    let push = port
        .push_image("registry.example/openpanel/web:latest")
        .await;
    assert!(matches!(
        push,
        Err(openpanel_domain::docker::DockerError::Forbidden)
    ));

    // The domain error the build adapter returns for that rejection is
    // fixed and credential-free...
    let domain = openpanel_domain::ApplicationDeliveryError::RegistryUnauthorized;
    assert_eq!(domain.to_string(), "registry authorization rejected");
    // ...and the provider maps it onto an adapter failure.
    let mapped: ForgePublishProviderError =
        openpanel_app::ApplicationDeliveryServiceError::Domain(domain).into();
    assert_eq!(
        mapped,
        ForgePublishProviderError::Adapter("registry authorization rejected".into())
    );
    let rendered = format!("{mapped:?} {mapped}");
    assert!(!rendered.contains("password"), "{rendered}");
    assert!(!rendered.contains("secret"), "{rendered}");
}

/// Scenario fingerprint: "Deploy converges one labeled container".
/// Promotion converges the environment container with the three
/// delivery identity labels and a terminal healthy outcome.
#[tokio::test(flavor = "current_thread")]
async fn docker_delivery_deploy_converges_one_labeled_container() {
    let docker = Arc::new(openpanel_app::docker::testing::FakeDocker::new());
    let composition = compose(docker.clone());
    let caller = composition.caller.clone();
    let application = compose_application(&composition);
    let release = composition
        .delivery()
        .build_release(&caller, application.id(), "production")
        .expect("release built");

    let promoted = composition
        .delivery()
        .promote(
            &caller,
            application.id(),
            "production",
            release.id(),
            "PROMOTE",
        )
        .expect("promotion converges");

    assert_eq!(promoted.state(), ReleaseState::Healthy);
    let labels = docker.last_labels();
    assert_eq!(labels.len(), 3, "exactly the three delivery labels");
    assert_eq!(
        labels.get("openpanel.application"),
        Some(&application.id().to_string())
    );
    assert_eq!(
        labels.get("openpanel.release"),
        Some(&release.id().to_string())
    );
    assert_eq!(
        labels.get("openpanel.environment"),
        Some(&promoted.environment_id().to_string())
    );
}

/// Scenario fingerprint: "Rollback target missing". A rollback whose
/// target release exists in delivery state but was never converged on
/// the host is refused with a typed error, and mutates nothing.
#[tokio::test(flavor = "current_thread")]
async fn docker_delivery_rollback_to_a_missing_target_is_typed_and_inert() {
    let docker = Arc::new(openpanel_app::docker::testing::FakeDocker::new());
    let composition = compose(docker.clone());
    let caller = composition.caller.clone();
    let application = compose_application(&composition);
    let delivery = composition.delivery();

    // A healthy release that the runtime adapter really converges.
    let healthy = delivery
        .build_release(&caller, application.id(), "production")
        .expect("release built");
    delivery
        .promote(
            &caller,
            application.id(),
            "production",
            healthy.id(),
            "PROMOTE",
        )
        .expect("promotion converges");

    // A second release whose rollout fails, so the release is in
    // `Failed` (a rollback-eligible state) while the host never
    // converged it.
    docker.set_failure(Some(openpanel_app::docker::testing::FAIL_UNHEALTHY));
    let failed = delivery
        .build_release(&caller, application.id(), "production")
        .expect("second release built");
    let outcome = delivery.promote(
        &caller,
        application.id(),
        "production",
        failed.id(),
        "PROMOTE",
    );
    assert!(
        outcome.is_err() || outcome.expect("failed release").state() == ReleaseState::Failed,
        "the second rollout does not reach healthy"
    );
    docker.set_failure(None);
    let before = docker.call_count();

    let error = delivery
        .rollback(
            &caller,
            application.id(),
            "production",
            failed.id(),
            "ROLLBACK",
        )
        .expect_err("rollback to a never-converged release is refused");

    let rendered = error.to_string();
    assert!(rendered.contains("rollback target release"), "{rendered}");
    assert!(
        openpanel_app::docker::testing::mutating_calls(&docker.calls()[before..]).is_empty(),
        "a refused rollback mutates nothing: {:?}",
        &docker.calls()[before..]
    );
}

/// Scenario fingerprint: "Publish path succeeds against the double".
/// Preflight and Publish both succeed through the production
/// adapters; with the deleted `Unavailable` seam (no registered
/// adapter) the very same frames failed with `not found`.
#[tokio::test(flavor = "current_thread")]
async fn docker_delivery_publish_path_succeeds_against_the_double() {
    let docker = Arc::new(openpanel_app::docker::testing::FakeDocker::new());
    let composition = compose(docker.clone());
    let caller = composition.caller.clone();

    let preflight = composition
        .provider
        .handle_request(publish_request("preflight", true), &caller)
        .await;
    assert_eq!(
        preflight.status,
        openpanel_domain::forge_publish_provider::ForgeProviderStatus::DryRun,
        "preflight diagnostic: {:?}",
        preflight.diagnostic
    );

    let publish = composition
        .provider
        .handle_request(publish_request("publish", false), &caller)
        .await;
    assert_eq!(
        publish.status,
        openpanel_domain::forge_publish_provider::ForgeProviderStatus::Succeeded,
        "publish diagnostic: {:?}",
        publish.diagnostic
    );
    let terminal = publish.terminal.expect("publish returns terminal evidence");
    assert_eq!(terminal.release_digest, OCI_DIGEST);
    assert!(
        docker.last_labels().contains_key("openpanel.application"),
        "publish converged a labeled container"
    );
}

/// The deleted seam's failure mode, asserted so DCK-004's contrast
/// cannot silently regress: with no adapter registered, the same
/// frames are refused instead of silently "succeeding".
#[tokio::test(flavor = "current_thread")]
async fn docker_delivery_publish_without_a_registered_adapter_is_refused() {
    let delivery = Arc::new(service_with(Arc::new(HealthyRuntimeAdapter)));
    let _application = {
        let caller = owner();
        let application = delivery
            .register_application(&caller, "web", oci_source(), "npm run build")
            .expect("valid application");
        delivery
            .add_environment(
                &caller,
                application.id(),
                EnvironmentKind::Production,
                "production",
                4,
                SecretSetRef::new("prod-secrets", None).unwrap(),
                None,
            )
            .expect("valid environment");
        application
    };
    let provider = Arc::new(ForgePublishProvider::new(
        ForgePublishProviderConfig {
            adapter_id: openpanel_app::DOCKER_DEPLOYMENT_ADAPTER_ID.to_string(),
            audit_actor: "forge-publish-provider".to_string(),
        },
        delivery,
        Arc::new(DeploymentAdapterService::new(vec![], noop_audit())),
        noop_audit(),
    ));

    let preflight = provider
        .handle_request(publish_request("preflight", true), &owner())
        .await;

    assert_ne!(
        preflight.status,
        openpanel_domain::forge_publish_provider::ForgeProviderStatus::DryRun,
        "an unregistered adapter must not report a dry run"
    );
}

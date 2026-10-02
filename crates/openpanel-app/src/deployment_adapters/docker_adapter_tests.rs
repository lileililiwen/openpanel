//! Behavioral tests for the production deployment adapter.

use std::sync::Arc;

use chrono::Utc;
use openpanel_domain::deployment_adapters::{
    DeploymentAction, DeploymentAdapter, DeploymentEvidence, DeploymentPlan, DeploymentState,
    OperationKey,
};

use crate::application_delivery::testing::{FixedTargetResolver, target_for};
use crate::application_delivery::{ConvergeRegistry, DockerRuntimeAdapter};
use crate::docker::testing::{FakeDocker, FakeNetwork, mutating_calls};

use super::{DOCKER_DEPLOYMENT_ADAPTER_ID, DockerDeploymentAdapter};

const NAMESPACE: &str = "registry.example/openpanel";
const DIGEST: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

fn runtime(docker: Arc<FakeDocker>) -> DockerRuntimeAdapter {
    DockerRuntimeAdapter::new(
        docker,
        Arc::new(FakeNetwork::new()),
        NAMESPACE,
        Arc::new(ConvergeRegistry::new()),
    )
    .expect("valid config")
}

fn converge_target() -> crate::application_delivery::ConvergeTarget {
    target_for("web", "production", DIGEST)
}

fn adapter(docker: Arc<FakeDocker>, resolver: Arc<FixedTargetResolver>) -> DockerDeploymentAdapter {
    DockerDeploymentAdapter::new(Arc::new(runtime(docker)), resolver).expect("valid adapter")
}

/// Resolver that answers every forge plan target.
fn resolving() -> Arc<FixedTargetResolver> {
    Arc::new(FixedTargetResolver::with_target(converge_target()))
}

fn plan(action: DeploymentAction, adapter_id: &str, dry_run: bool) -> DeploymentPlan {
    let target = "forge:acme/web:production".to_string();
    DeploymentPlan::new(
        target,
        action,
        action.is_mutating().then(|| {
            OperationKey::new("forge:acme/web:production", action, "operation-1")
                .expect("valid operation key")
        }),
        DIGEST,
        Vec::new(),
        adapter_id,
        "publisher",
        dry_run,
    )
    .expect("valid plan")
}

#[test]
fn test_manifest_declares_docker_deploy_id_and_dry_run_capability() {
    let adapter = adapter(Arc::new(FakeDocker::new()), resolving());
    let manifest = adapter.manifest();
    assert_eq!(manifest.id(), DOCKER_DEPLOYMENT_ADAPTER_ID);
    assert_eq!(manifest.id(), "docker-deploy-v1");
    assert_eq!(manifest.target_scheme(), "forge:");
    // Dry-run is expressed as the preflight action the provider uses.
    assert!(manifest.supports(DeploymentAction::Preflight));
    assert!(manifest.supports(DeploymentAction::Deploy));
    assert!(manifest.supports(DeploymentAction::Rollback));
    assert!(manifest.supports_rollback());
}

#[test]
fn test_foreign_plan_is_refused_without_touching_the_host() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), resolving());
    let foreign = plan(DeploymentAction::Deploy, "mac-jenkins-v1", false);

    let evidence = adapter.execute(&foreign, None);

    assert_eq!(evidence.state(), DeploymentState::PreflightRejected);
    assert!(evidence.diagnostic().contains("mac-jenkins-v1"));
    assert!(
        docker.calls().is_empty(),
        "host untouched: {:?}",
        docker.calls()
    );
}

#[test]
fn test_dry_run_records_zero_mutations() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), resolving());
    let dry = plan(
        DeploymentAction::Preflight,
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        true,
    );

    let evidence = adapter.execute(&dry, None);

    assert!(evidence.state().is_terminal());
    assert_eq!(evidence.state(), DeploymentState::Ready);
    assert!(
        mutating_calls(&docker.calls()).is_empty(),
        "dry-run must not mutate: {:?}",
        docker.calls()
    );
    // The full read-only converge check still ran.
    assert_eq!(docker.calls(), vec!["ping", "image_exists"]);
}

#[test]
fn test_replay_returns_cached_evidence_verbatim() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), resolving());
    let deploy = plan(
        DeploymentAction::Deploy,
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        false,
    );
    let first = adapter.execute(&deploy, None);
    let before = docker.call_count();

    let replay = adapter.execute(&deploy, Some(&first));

    assert_eq!(replay, first, "cached evidence returned verbatim");
    assert_eq!(
        docker.call_count(),
        before,
        "replay performs no host operation"
    );
}

#[test]
fn test_deploy_converges_and_records_ready_evidence() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), resolving());
    let deploy = plan(
        DeploymentAction::Deploy,
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        false,
    );

    let evidence = adapter.execute(&deploy, None);

    assert_eq!(evidence.state(), DeploymentState::Ready);
    assert_eq!(evidence.adapter(), DOCKER_DEPLOYMENT_ADAPTER_ID);
    assert!(!mutating_calls(&docker.calls()).is_empty());
}

#[test]
fn test_unresolvable_target_rejects_a_preflight_and_fails_a_deploy() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), Arc::new(FixedTargetResolver::new()));

    let preflight = adapter.execute(
        &plan(
            DeploymentAction::Preflight,
            DOCKER_DEPLOYMENT_ADAPTER_ID,
            true,
        ),
        None,
    );
    let deploy = adapter.execute(
        &plan(
            DeploymentAction::Deploy,
            DOCKER_DEPLOYMENT_ADAPTER_ID,
            false,
        ),
        None,
    );

    assert_eq!(preflight.state(), DeploymentState::PreflightRejected);
    assert_eq!(deploy.state(), DeploymentState::Failed);
    assert!(docker.calls().is_empty());
}

#[test]
fn test_target_outside_the_forge_namespace_is_refused() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), resolving());
    let foreign = DeploymentPlan::new(
        "systemd:web-01",
        DeploymentAction::Preflight,
        Some(
            OperationKey::new("systemd:web-01", DeploymentAction::Preflight, "op-1").expect("key"),
        ),
        DIGEST,
        Vec::new(),
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        "publisher",
        true,
    )
    .expect("valid plan");

    let evidence = adapter.execute(&foreign, None);

    assert_eq!(evidence.state(), DeploymentState::PreflightRejected);
    assert!(evidence.diagnostic().contains("forge:"));
    assert!(docker.calls().is_empty());
}

#[test]
fn test_daemon_outage_fails_the_deploy_with_a_typed_diagnostic() {
    let docker = Arc::new(FakeDocker::new().with_failure("daemon"));
    let adapter = adapter(docker.clone(), resolving());
    let deploy = plan(
        DeploymentAction::Deploy,
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        false,
    );

    let evidence = adapter.execute(&deploy, None);

    assert_eq!(evidence.state(), DeploymentState::Failed);
    assert_eq!(evidence.diagnostic(), "docker runtime unavailable");
    assert!(!evidence.diagnostic().contains("secret"));
}

#[test]
fn test_read_only_status_action_never_mutates() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker.clone(), resolving());
    let status = plan(
        DeploymentAction::Status,
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        false,
    );

    let evidence = adapter.execute(&status, None);

    assert_eq!(evidence.state(), DeploymentState::Ready);
    assert!(mutating_calls(&docker.calls()).is_empty());
}

#[test]
fn test_evidence_diagnostic_is_bounded() {
    let docker = Arc::new(FakeDocker::new());
    let adapter = adapter(docker, resolving());
    let deploy = plan(
        DeploymentAction::Deploy,
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        false,
    );

    let evidence: DeploymentEvidence = adapter.execute(&deploy, None);

    assert!(evidence.diagnostic().len() <= 1024);
    assert_eq!(evidence.completed_at(), evidence.started_at());
    let _ = Utc::now();
}

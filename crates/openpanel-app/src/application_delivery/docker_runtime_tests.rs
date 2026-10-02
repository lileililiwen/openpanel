//! Behavioral tests for the production runtime adapter.

use std::sync::Arc;

use chrono::Utc;
use openpanel_domain::{
    ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind, Environment,
    EnvironmentKind, Release, ReleaseDigest, ReleaseState, RuntimeAdapter, SecretSetRef,
};
use uuid::Uuid;

use super::docker_runtime::{ConvergeRegistry, ConvergeTarget, DockerRuntimeAdapter};
use crate::docker::testing::{FAIL_DAEMON, FAIL_UNHEALTHY, FakeDocker, FakeNetwork};

const NAMESPACE: &str = "registry.example/openpanel";

struct Fixture {
    network: Arc<FakeNetwork>,
    runtime: DockerRuntimeAdapter,
    release: Release,
    environment: Environment,
}

fn fixture(docker: Arc<FakeDocker>) -> Fixture {
    let network = Arc::new(FakeNetwork::new());
    let runtime = DockerRuntimeAdapter::new(
        docker.clone(),
        network.clone(),
        NAMESPACE,
        Arc::new(ConvergeRegistry::new()),
    )
    .expect("valid config");
    let application_id = Uuid::new_v4();
    let environment = Environment::new(
        Uuid::new_v4(),
        application_id,
        EnvironmentKind::Production,
        "production",
        4,
        SecretSetRef::new("prod-secrets", None).expect("valid secret set"),
        Utc::now(),
        None,
    )
    .expect("valid environment");
    let release = Release::new(
        Uuid::new_v4(),
        application_id,
        environment.id(),
        ReleaseDigest::new(crate::docker::testing::PUSHED_DIGEST, Utc::now())
            .expect("immutable digest"),
        ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/acme/web.git",
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b",
        )
        .expect("valid source"),
        Utc::now(),
        None,
    );
    let _ = docker;
    Fixture {
        network,
        runtime,
        release,
        environment,
    }
}

fn target(fixture: &Fixture) -> ConvergeTarget {
    ConvergeTarget::from_release(&fixture.release, &fixture.environment).expect("converge target")
}

#[test]
fn test_deploy_converges_container_with_the_three_labels() {
    let docker = Arc::new(FakeDocker::new());
    let fixture = fixture(docker.clone());
    let release = fixture.release.id();
    let environment = fixture.environment.id();
    let application = fixture.release.application_id();

    let outcome = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("deploy converges");

    assert_eq!(outcome.state, ReleaseState::Healthy);
    let labels = docker.last_labels();
    assert_eq!(
        labels.get("openpanel.application"),
        Some(&application.to_string())
    );
    assert_eq!(
        labels.get("openpanel.environment"),
        Some(&environment.to_string())
    );
    assert_eq!(labels.get("openpanel.release"), Some(&release.to_string()));
    assert_eq!(labels.len(), 3, "exactly the three delivery labels");
    // The image is pinned by digest, never by a mutable tag.
    assert_eq!(
        docker
            .calls()
            .iter()
            .find(|call| call.starts_with("image:"))
            .map(String::as_str),
        Some(
            format!(
                "image:{NAMESPACE}/{application}@{}",
                crate::docker::testing::PUSHED_DIGEST
            )
            .as_str()
        )
    );
    assert_eq!(fixture.network.calls(), vec!["ensure"]);
}

#[test]
fn test_deploy_pulls_the_pinned_digest_when_the_image_is_absent() {
    let docker = Arc::new(FakeDocker::new().without_image());
    let fixture = fixture(docker.clone());
    let outcome = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("deploy converges");
    assert_eq!(outcome.state, ReleaseState::Healthy);
    assert_eq!(
        docker.calls().first().map(String::as_str),
        Some("image_exists")
    );
    assert!(docker.calls().iter().any(|call| call == "pull"));
}

#[test]
fn test_health_verdict_is_honored_over_a_running_status() {
    // Running + unhealthy verdict is a terminal failure, and the
    // half-deployed container is discarded rather than left behind.
    let docker = Arc::new(FakeDocker::new().with_failure(FAIL_UNHEALTHY));
    let fixture = fixture(docker.clone());
    let outcome = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("deploy reports a terminal outcome");
    assert_eq!(outcome.state, ReleaseState::Failed);
    assert!(
        docker
            .calls()
            .iter()
            .any(|call| call == "remove:container-id:true")
    );
}

#[test]
fn test_healthy_verdict_without_healthcheck_is_accepted() {
    // No HEALTHCHECK declared: a running container satisfies the
    // policy, so the release is not punished for having no health
    // check to report.
    let docker = Arc::new(FakeDocker::new());
    let fixture = fixture(docker.clone());
    let outcome = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("deploy converges");
    assert_eq!(outcome.state, ReleaseState::Healthy);
    assert_eq!(outcome.diagnostic, "container is running");
}

#[test]
fn test_exited_container_is_a_terminal_failure() {
    let docker = Arc::new(FakeDocker::new().with_status("exited"));
    let fixture = fixture(docker.clone());
    let outcome = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("deploy reports a terminal outcome");
    assert_eq!(outcome.state, ReleaseState::Failed);
    assert!(outcome.diagnostic.contains("exited"));
}

#[test]
fn test_daemon_unavailable_is_a_typed_redacted_error() {
    let docker = Arc::new(FakeDocker::new().with_failure(FAIL_DAEMON));
    let fixture = fixture(docker.clone());
    let result = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment);
    assert_eq!(result, Err(ApplicationDeliveryError::RuntimeUnavailable));
    let message = result.expect_err("daemon down").to_string();
    assert!(!message.contains("secret"));
    assert!(!message.contains("/run/docker.sock"));
}

#[test]
fn test_rollback_to_an_unknown_release_is_typed_and_mutates_nothing() {
    let docker = Arc::new(FakeDocker::new());
    let fixture = fixture(docker.clone());
    let result = fixture
        .runtime
        .rollback(&fixture.release, &fixture.environment, Uuid::new_v4());
    assert_eq!(
        result,
        Err(ApplicationDeliveryError::IllegalTransition(
            "rollback target release was never converged by this provider".into()
        ))
    );
    assert!(
        docker.calls().is_empty(),
        "host untouched: {:?}",
        docker.calls()
    );
}

#[test]
fn test_rollback_reconverges_to_a_converged_release() {
    let docker = Arc::new(FakeDocker::new());
    let fixture = fixture(docker.clone());
    fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("first converge");
    let prior = fixture.release.id();
    // A second release for the same environment, failed on the host.
    let failing = Release::new(
        Uuid::new_v4(),
        fixture.release.application_id(),
        fixture.environment.id(),
        ReleaseDigest::new(
            "sha256:1f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            Utc::now(),
        )
        .expect("immutable digest"),
        fixture.release.source().clone(),
        Utc::now(),
        Some(prior),
    );

    let outcome = fixture
        .runtime
        .rollback(&failing, &fixture.environment, prior)
        .expect("rollback converges to the prior release");

    assert_eq!(outcome.state, ReleaseState::Healthy);
    let last_release_label = docker
        .calls()
        .iter()
        .rev()
        .find(|call| call.starts_with("label:openpanel.release="))
        .cloned()
        .expect("release label recorded");
    assert_eq!(
        last_release_label,
        format!("label:openpanel.release={prior}")
    );
}

#[test]
fn test_rollback_target_from_another_environment_is_refused() {
    let docker = Arc::new(FakeDocker::new());
    let fixture = fixture(docker.clone());
    fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("first converge");
    // A foreign environment's release id, converged under a
    // different environment id.
    let foreign_environment = Environment::new(
        Uuid::new_v4(),
        fixture.release.application_id(),
        EnvironmentKind::Preview,
        "preview-1",
        2,
        SecretSetRef::new("preview-secrets", None).expect("valid secret set"),
        Utc::now(),
        Some(Utc::now() + chrono::Duration::hours(1)),
    )
    .expect("valid environment");
    let foreign_release = Release::new(
        Uuid::new_v4(),
        fixture.release.application_id(),
        foreign_environment.id(),
        ReleaseDigest::new(
            "sha256:2f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            Utc::now(),
        )
        .expect("immutable digest"),
        fixture.release.source().clone(),
        Utc::now(),
        None,
    );
    fixture.runtime.registry().record(
        &ConvergeTarget::from_release(&foreign_release, &foreign_environment)
            .expect("foreign target"),
    );
    let before = docker.call_count();

    let result =
        fixture
            .runtime
            .rollback(&fixture.release, &fixture.environment, foreign_release.id());

    assert_eq!(
        result,
        Err(ApplicationDeliveryError::IllegalTransition(
            "rollback target release belongs to another environment".into()
        ))
    );
    assert_eq!(docker.call_count(), before, "host untouched");
}

#[test]
fn test_diagnostics_are_bounded_and_carry_no_environment_or_credential() {
    let docker = Arc::new(FakeDocker::new().with_status(&"x".repeat(4096)));
    let fixture = fixture(docker.clone());
    let outcome = fixture
        .runtime
        .deploy(&fixture.release, &fixture.environment)
        .expect("deploy reports a terminal outcome");
    assert!(outcome.diagnostic.len() <= 1024, "diagnostic is bounded");
    assert!(!outcome.diagnostic.contains("password"));
    assert!(!outcome.diagnostic.contains("prod-secrets"));
}

#[test]
fn test_converge_target_refuses_a_mutable_digest() {
    let result = ConvergeTarget::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "web",
        "production",
        Uuid::new_v4(),
        "latest",
    );
    assert!(matches!(
        result,
        Err(ApplicationDeliveryError::MutableDigest(_))
    ));
}

#[test]
fn test_read_only_check_reports_a_missing_image_without_mutating() {
    let docker = Arc::new(FakeDocker::new().without_image());
    let fixture = fixture(docker.clone());
    let outcome = fixture
        .runtime
        .check(&target(&fixture))
        .expect("check completes");
    assert_eq!(outcome.state, ReleaseState::Failed);
    assert!(crate::docker::testing::mutating_calls(&docker.calls()).is_empty());
}

#[test]
fn test_invalid_namespace_is_refused_at_composition() {
    for namespace in ["", "   ", "registry.example@openpanel", "registry example"] {
        assert!(
            DockerRuntimeAdapter::new(
                Arc::new(FakeDocker::new()),
                Arc::new(FakeNetwork::new()),
                namespace,
                Arc::new(ConvergeRegistry::new()),
            )
            .is_err(),
            "namespace {namespace:?} must be refused"
        );
    }
}

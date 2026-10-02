//! Behavioral tests for the production build adapter.

use std::sync::Arc;

use openpanel_domain::{
    ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind, BuildAdapter,
};
use uuid::Uuid;

use super::DockerBuildAdapter;
use crate::docker::testing::{FAIL_AUTH, FAIL_DAEMON, FakeDocker, PUSHED_DIGEST};

const NAMESPACE: &str = "registry.example/openpanel";

fn adapter(docker: Arc<FakeDocker>) -> DockerBuildAdapter {
    DockerBuildAdapter::new(docker, NAMESPACE.into(), 1024 * 1024).expect("valid config")
}

fn oci_source(digest: &str) -> ApplicationSource {
    ApplicationSource::new(
        ApplicationSourceKind::Oci,
        "oci://registry.example/openpanel/app",
        digest,
    )
    .expect("valid source")
}

/// Create a local git repository with a Dockerfile and return its
/// `file://` source at the commit sha. The `TempDir` is returned with
/// it: the adapter fetches during `resolve_digest`, so the caller must
/// keep the source repository alive for the whole call.
fn git_fixture() -> (tempfile::TempDir, ApplicationSource) {
    use std::{fs, process::Command};
    let repo = tempfile::tempdir().expect("temporary repository");
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(args)
            .status()
            .expect("git available");
        assert!(status.success(), "git command failed: {args:?}");
    };
    run(&["init", "--quiet"]);
    // A real forge host (GitHub, GitLab) allows fetching the exact
    // commit an immutable release names; the fixture repository
    // emulates that upload-pack contract so a shallow fetch by sha
    // is exercised.
    run(&["config", "uploadpack.allowAnySHA1InWant", "true"]);
    fs::write(
        repo.path().join("Dockerfile"),
        "FROM scratch\nARG OPENPANEL_BUILD_COMMAND\n",
    )
    .expect("Dockerfile written");
    run(&["add", "Dockerfile"]);
    run(&[
        "-c",
        "user.name=OpenPanel Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "--quiet",
        "-m",
        "fixture",
    ]);
    let output = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("sha available");
    let sha = String::from_utf8(output.stdout)
        .expect("utf8 sha")
        .trim()
        .to_string();
    let source = ApplicationSource::new(
        ApplicationSourceKind::Git,
        format!("file://{}", repo.path().display()),
        sha,
    )
    .expect("valid source");
    (repo, source)
}

/// A remote-shaped source that the adapter must refuse because it
/// carries credentials in the authority.
fn credentialed_source() -> ApplicationSource {
    ApplicationSource::new(
        ApplicationSourceKind::Git,
        "https://user:secret@example.com/acme/web.git",
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b",
    )
    .expect("valid source")
}

#[test]
fn test_oci_source_verifies_pinned_digest_without_build_or_push() {
    let docker = Arc::new(FakeDocker::new());
    let digest = PUSHED_DIGEST;
    let resolved = adapter(docker.clone())
        .resolve_digest(Uuid::new_v4(), &oci_source(digest), "")
        .expect("OCI present");
    assert_eq!(resolved.value(), digest);
    assert_eq!(docker.calls(), vec!["ping", "image_exists"]);
}

#[test]
fn test_oci_source_pulls_when_the_image_is_absent() {
    let docker = Arc::new(FakeDocker::new().without_image());
    let resolved = adapter(docker.clone())
        .resolve_digest(Uuid::new_v4(), &oci_source(PUSHED_DIGEST), "")
        .expect("OCI pullable");
    assert_eq!(resolved.value(), PUSHED_DIGEST);
    assert_eq!(docker.calls(), vec!["ping", "image_exists", "pull"]);
}

#[test]
fn test_daemon_and_auth_errors_are_typed_and_redacted() {
    let daemon = adapter(Arc::new(FakeDocker::new().with_failure(FAIL_DAEMON)));
    let result = daemon.resolve_digest(Uuid::new_v4(), &oci_source(PUSHED_DIGEST), "");
    assert_eq!(result, Err(ApplicationDeliveryError::RuntimeUnavailable));
    assert!(
        !ApplicationDeliveryError::RuntimeUnavailable
            .to_string()
            .contains("secret")
    );
}

#[test]
fn test_git_build_push_returns_registry_digest_and_preserves_command() {
    let docker = Arc::new(FakeDocker::new());
    let (_repo, source) = git_fixture();
    let resolved = adapter(docker.clone())
        .resolve_digest(Uuid::new_v4(), &source, "npm ci && npm run build")
        .expect("build and push");
    assert_eq!(resolved.value(), PUSHED_DIGEST);
    assert_eq!(
        docker.calls(),
        vec![
            "ping",
            "build_image",
            "npm ci && npm run build",
            "push_image"
        ]
    );
}

#[test]
fn test_registry_authorization_rejection_is_redacted() {
    let docker = Arc::new(FakeDocker::new().with_failure(FAIL_AUTH));
    let (_repo, source) = git_fixture();
    let result = adapter(docker).resolve_digest(Uuid::new_v4(), &source, "");
    assert_eq!(result, Err(ApplicationDeliveryError::RegistryUnauthorized));
    let message = result.expect_err("auth failure").to_string();
    assert!(!message.contains("password"));
    assert_eq!(message, "registry authorization rejected");
}

#[test]
fn test_credentialed_git_url_is_refused() {
    // The adapter never accepts credentials embedded in the URL.
    let source = credentialed_source();
    let docker = Arc::new(FakeDocker::new());
    let result = adapter(docker.clone()).resolve_digest(Uuid::new_v4(), &source, "");
    assert!(matches!(result, Err(ApplicationDeliveryError::Invalid(_))));
    assert_eq!(docker.calls(), vec!["ping"]);
}

#[test]
fn test_build_context_size_ceiling_is_enforced() {
    let docker = Arc::new(FakeDocker::new());
    let bounded =
        DockerBuildAdapter::new(docker.clone(), NAMESPACE.into(), 8).expect("valid config");
    let (_repo, source) = git_fixture();
    let result = bounded.resolve_digest(Uuid::new_v4(), &source, "");
    assert!(matches!(result, Err(ApplicationDeliveryError::Invalid(_))));
    // The oversized context never reached the engine.
    assert!(!docker.calls().iter().any(|call| call == "build_image"));
}

#[test]
fn test_invalid_namespace_is_refused_at_composition() {
    for namespace in ["", "   ", "registry.example@openpanel", "registry example"] {
        assert!(
            DockerBuildAdapter::new(Arc::new(FakeDocker::new()), namespace.into(), 1024).is_err(),
            "namespace {namespace:?} must be refused"
        );
    }
}

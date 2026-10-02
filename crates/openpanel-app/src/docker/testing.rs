//! Recording docker-port test double.
//!
//! One double serves every suite that needs a docker port without a
//! daemon: the delivery build / runtime / deployment adapter unit
//! tests, the application-delivery integration suite, and the forge
//! provider CLI suite. It records every verb (including the delivery
//! identity labels), can be pinned to a failure mode or an engine
//! state, and is the *only* docker double in the workspace so the
//! suites cannot drift apart.
//!
//! It lives here, not in the test-support crate, because it
//! implements this crate's own port: a double in a crate that depends
//! on `openpanel-app` would not satisfy the trait the unit tests see.
//!
//! No production path composes a double: only tests do.

// Test-double setup may fail fast: an unbuildable double is a
// broken test environment, not a production condition. This mirrors
// the crate-level allow in `openpanel-test-support`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use openpanel_domain::docker::{ComposeStack, ContainerSpec, DockerError};

use super::{ApplyReport, DockerAdapter, ExecResult, NetworkAdapter, RuntimeContainerState};

/// Push digest the double reports. Immutable `sha256:` value.
pub const PUSHED_DIGEST: &str =
    "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

/// Failure modes the double can be pinned to.
pub const FAIL_DAEMON: &str = "daemon";
/// Push returns a registry auth rejection.
pub const FAIL_AUTH: &str = "auth";
/// The engine reports the container as unhealthy.
pub const FAIL_UNHEALTHY: &str = "unhealthy";

/// Recording docker double.
pub struct FakeDocker {
    calls: Mutex<Vec<String>>,
    failure: Mutex<Option<&'static str>>,
    image_present: AtomicBool,
    state: Mutex<RuntimeContainerState>,
}

impl FakeDocker {
    /// Double with the image present and a running container.
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            failure: Mutex::new(None),
            image_present: AtomicBool::new(true),
            state: Mutex::new(RuntimeContainerState {
                status: "running".into(),
                oom_killed: false,
                health: None,
            }),
        }
    }

    /// Pin the double to a failure mode.
    pub fn with_failure(self, failure: &'static str) -> Self {
        self.set_failure(Some(failure));
        self
    }

    /// Change the pinned failure mode mid-test, so a suite can fail
    /// one rollout and succeed the next on the same double.
    pub fn set_failure(&self, failure: Option<&'static str>) {
        *self.failure.lock().expect("fake failure") = failure;
    }

    /// Report the pinned image as absent so the adapter must pull.
    pub fn without_image(self) -> Self {
        self.image_present.store(false, Ordering::SeqCst);
        self
    }

    /// Pin the engine-reported status.
    pub fn with_status(self, status: &str) -> Self {
        self.state.lock().expect("fake state").status = status.into();
        self
    }

    /// Pin the engine-reported health verdict.
    pub fn with_health(self, health: &str) -> Self {
        self.state.lock().expect("fake state").health = Some(health.into());
        self
    }

    /// Ordered call log. Labels are recorded as `label:key=value`.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("fake calls").clone()
    }

    /// Number of recorded calls, used to prove a dry-run mutated
    /// nothing.
    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("fake calls").len()
    }

    /// The delivery identity labels of the last labeled create.
    pub fn last_labels(&self) -> BTreeMap<String, String> {
        self.calls
            .lock()
            .expect("fake calls")
            .iter()
            .filter_map(|call| call.strip_prefix("label:"))
            .filter_map(|entry| entry.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn record(&self, call: &str) {
        self.calls.lock().expect("fake calls").push(call.into());
    }

    /// A daemon that is down answers no verb at all, so a caller that
    /// skips its liveness probe still observes the outage.
    fn unavailable(&self) -> Option<DockerError> {
        self.is_failing(FAIL_DAEMON)
            .then(|| DockerError::Adapter("socket /run/docker.sock: secret".into()))
    }

    /// Whether the double is currently pinned to `mode`.
    fn is_failing(&self, mode: &'static str) -> bool {
        *self.failure.lock().expect("fake failure") == Some(mode)
    }
}

impl Default for FakeDocker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DockerAdapter for FakeDocker {
    async fn ping(&self) -> Result<(), DockerError> {
        self.record("ping");
        match self.unavailable() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn pull(&self, image: &str) -> Result<String, DockerError> {
        self.record("pull");
        if let Some(error) = self.unavailable() {
            return Err(error);
        }
        self.image_present.store(true, Ordering::SeqCst);
        Ok(image
            .rsplit('@')
            .next()
            .unwrap_or(PUSHED_DIGEST)
            .to_string())
    }

    async fn image_exists(&self, _: &str) -> Result<bool, DockerError> {
        self.record("image_exists");
        if let Some(error) = self.unavailable() {
            return Err(error);
        }
        Ok(self.image_present.load(Ordering::SeqCst))
    }

    async fn build_image(
        &self,
        context: Vec<u8>,
        _: &str,
        command: &str,
    ) -> Result<(), DockerError> {
        self.record("build_image");
        if let Some(error) = self.unavailable() {
            return Err(error);
        }
        self.record(command);
        let mut archive = tar::Archive::new(context.as_slice());
        let names: Vec<String> = archive
            .entries()
            .expect("valid tar")
            .map(|entry| {
                entry
                    .expect("valid entry")
                    .path()
                    .expect("valid path")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(names.contains(&"Dockerfile".to_string()));
        assert!(!names.iter().any(|name| name.starts_with(".git/")));
        Ok(())
    }

    async fn push_image(&self, _: &str) -> Result<String, DockerError> {
        self.record("push_image");
        if let Some(error) = self.unavailable() {
            return Err(error);
        }
        if self.is_failing(FAIL_AUTH) {
            return Err(DockerError::Forbidden);
        }
        Ok(PUSHED_DIGEST.into())
    }

    async fn create(&self, _: &ContainerSpec) -> Result<String, DockerError> {
        self.record("create");
        Ok("container-id".into())
    }

    async fn create_labeled(
        &self,
        spec: &ContainerSpec,
        labels: &BTreeMap<String, String>,
    ) -> Result<String, DockerError> {
        self.record("create_labeled");
        if let Some(error) = self.unavailable() {
            return Err(error);
        }
        self.record(&format!("image:{}", spec.image));
        for (key, value) in labels {
            self.record(&format!("label:{key}={value}"));
        }
        Ok("container-id".into())
    }

    async fn start(&self, _: &str) -> Result<(), DockerError> {
        self.record("start");
        match self.unavailable() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn stop(&self, _: &str) -> Result<(), DockerError> {
        self.record("stop");
        Ok(())
    }

    async fn restart(&self, _: &str) -> Result<(), DockerError> {
        self.record("restart");
        Ok(())
    }

    async fn remove(&self, id: &str, force: bool) -> Result<(), DockerError> {
        self.record(&format!("remove:{id}:{force}"));
        match self.unavailable() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn inspect(&self, _: &str) -> Result<RuntimeContainerState, DockerError> {
        self.record("inspect");
        if let Some(error) = self.unavailable() {
            return Err(error);
        }
        let mut state = self.state.lock().expect("fake state").clone();
        if self.is_failing(FAIL_UNHEALTHY) {
            state.health = Some("unhealthy".into());
        }
        Ok(state)
    }

    async fn logs(&self, _: &str, _: u64) -> Result<Vec<String>, DockerError> {
        self.record("logs");
        Ok(Vec::new())
    }

    async fn exec(&self, _: &str, _: &[String], _: &str) -> Result<ExecResult, DockerError> {
        self.record("exec");
        Ok(ExecResult {
            exit_code: 0,
            output: String::new(),
            truncated: false,
        })
    }

    async fn apply_stack(&self, _: &ComposeStack) -> Result<ApplyReport, DockerError> {
        self.record("apply_stack");
        Ok(ApplyReport {
            created: Vec::new(),
            removed: Vec::new(),
        })
    }

    async fn remove_stack(&self, _: &ComposeStack) -> Result<Vec<String>, DockerError> {
        self.record("remove_stack");
        Ok(Vec::new())
    }
}

/// No-op network double: the delivery rollout only needs the system
/// bridge to exist.
pub struct FakeNetwork {
    calls: Mutex<Vec<String>>,
}

impl FakeNetwork {
    /// Empty network double.
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Ordered call log.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("fake network calls").clone()
    }
}

impl Default for FakeNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkAdapter for FakeNetwork {
    async fn ensure(&self, _: Option<uuid::Uuid>) -> Result<(), DockerError> {
        self.calls
            .lock()
            .expect("fake network calls")
            .push("ensure".into());
        Ok(())
    }

    async fn remove_if_unused(&self, _: Option<uuid::Uuid>) -> Result<(), DockerError> {
        self.calls
            .lock()
            .expect("fake network calls")
            .push("remove_if_unused".into());
        Ok(())
    }
}

/// Whether `calls` contains a host-mutating verb. Used to prove a
/// dry-run mutated nothing.
pub fn mutating_calls(calls: &[String]) -> Vec<&String> {
    const MUTATING: [&str; 5] = ["create", "create_labeled", "start", "stop", "restart"];
    calls
        .iter()
        .filter(|call| {
            MUTATING
                .iter()
                .any(|verb| call == verb || call.starts_with(&format!("{verb}:")))
        })
        .collect()
}

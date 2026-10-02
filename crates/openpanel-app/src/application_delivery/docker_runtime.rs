//! Docker-backed environment convergence for application delivery.
//!
//! The runtime adapter owns the host half of a release rollout: it
//! makes the pinned digest present in the engine, creates the
//! environment container with its delivery identity labels, starts
//! it, and projects the engine state onto a terminal
//! [`ReleaseOutcome`].
//!
//! Rollback re-converges to a prior release recorded in the shared
//! [`ConvergeRegistry`]. The registry is an in-memory projection of
//! "which release this provider actually converged"; it is replaced
//! by the SQLite state package rather than duplicated here, so a
//! restart degrades to a typed "no rollback target" error instead of
//! a silent host mutation.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use openpanel_domain::{
    ApplicationDeliveryError, Environment, Release, ReleaseOutcome, ReleaseState, RuntimeAdapter,
    User,
    application_delivery::redact_diagnostic,
    docker::{ContainerSpec, DockerError, ResourceLimits, RestartPolicy},
};
use uuid::Uuid;

use crate::docker::{DockerAdapter, NetworkAdapter, RuntimeContainerState};

use super::docker_build::run_docker;

/// Delivery identity labels written on every converged container.
/// The three labels are the whole rollout identity: the application,
/// the environment, and the release backing it.
const LABEL_APPLICATION: &str = "openpanel.application";
const LABEL_ENVIRONMENT: &str = "openpanel.environment";
const LABEL_RELEASE: &str = "openpanel.release";

/// Maximum diagnostic length carried by an outcome.
const MAX_DIAGNOSTIC_LEN: usize = 1024;

/// Default number of bounded health polls after a container starts.
const DEFAULT_HEALTH_POLLS: u32 = 5;

/// Pause between two health polls. The domain port is synchronous, so
/// the wait happens on the Docker worker thread, never on a Tokio
/// worker.
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// The concrete host target of one rollout: which container to run
/// and which immutable digest it must run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvergeTarget {
    application_id: Uuid,
    environment_id: Uuid,
    application_name: String,
    environment_name: String,
    release_id: Uuid,
    digest: String,
}

impl ConvergeTarget {
    /// Build a rollout target. The digest must be an immutable
    /// `sha256:` reference; a mutable tag can never back a release.
    pub fn new(
        application_id: Uuid,
        environment_id: Uuid,
        application_name: impl Into<String>,
        environment_name: impl Into<String>,
        release_id: Uuid,
        digest: impl Into<String>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let application_name = application_name.into();
        let environment_name = environment_name.into();
        let digest = digest.into();
        if application_name.trim().is_empty() || environment_name.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "converge target names are required".into(),
            ));
        }
        if !is_immutable_digest(&digest) {
            return Err(ApplicationDeliveryError::MutableDigest(
                "converge target requires an immutable sha256 digest".into(),
            ));
        }
        Ok(Self {
            application_id,
            environment_id,
            application_name,
            environment_name,
            release_id,
            digest,
        })
    }

    /// Build the target a domain release/environment pair converges to.
    pub fn from_release(
        release: &Release,
        environment: &Environment,
    ) -> Result<Self, ApplicationDeliveryError> {
        Self::new(
            release.application_id(),
            environment.id(),
            release.application_id().to_string(),
            environment.name(),
            release.id(),
            release.digest().value(),
        )
    }

    /// Owning application.
    pub fn application_id(&self) -> Uuid {
        self.application_id
    }

    /// Environment the container backs.
    pub fn environment_id(&self) -> Uuid {
        self.environment_id
    }

    /// Name of the application (the project slug tail the forge
    /// protocol publishes under).
    pub fn application_name(&self) -> &str {
        &self.application_name
    }

    /// Name of the environment (the plan target's environment).
    pub fn environment_name(&self) -> &str {
        &self.environment_name
    }

    /// Release the container runs.
    pub fn release_id(&self) -> Uuid {
        self.release_id
    }

    /// Immutable digest the container runs.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Digest-pinned image reference inside `namespace`.
    pub fn image_reference(&self, namespace: &str) -> String {
        format!("{namespace}/{}@{}", self.application_id, self.digest)
    }

    /// The three delivery identity labels.
    pub fn labels(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (LABEL_APPLICATION.into(), self.application_id.to_string()),
            (LABEL_ENVIRONMENT.into(), self.environment_id.to_string()),
            (LABEL_RELEASE.into(), self.release_id.to_string()),
        ])
    }

    /// Bounded, non-root container spec for the environment. The
    /// spec carries no environment variable and no bind mount: the
    /// delivery rollout owns neither, so the container starts from
    /// the image defaults under the panel's resource ceilings.
    pub fn container_spec(&self, image: String) -> ContainerSpec {
        ContainerSpec {
            id: self.environment_id,
            name: container_name(self.environment_id, self.release_id),
            image,
            site_id: None,
            env: BTreeMap::new(),
            command: Vec::new(),
            ports: Vec::new(),
            mounts: Vec::new(),
            capabilities: Vec::new(),
            limits: ResourceLimits::default(),
            restart_policy: RestartPolicy::UnlessStopped,
            user_namespace: "1000:1000".into(),
        }
    }
}

/// Projection of "which release this provider actually converged".
/// The runtime adapter writes it, `rollback` reads it, and the
/// deployment adapter receives the same shared handle so both
/// adapters agree on the host target of a plan.
#[derive(Debug, Default)]
pub struct ConvergeRegistry {
    converged: Mutex<BTreeMap<Uuid, Converged>>,
}

/// What this provider last converged for one environment.
#[derive(Debug, Clone)]
struct Converged {
    target: ConvergeTarget,
    container_id: String,
}

impl ConvergeRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the release and container currently backing an
    /// environment, returning the container it replaced. The caller
    /// retires that container so a release loop cannot accumulate
    /// containers on the host.
    fn replace(&self, target: &ConvergeTarget, container_id: &str) -> Option<String> {
        let mut guard = self.converged.lock().ok()?;
        let previous = guard
            .insert(
                target.environment_id(),
                Converged {
                    target: target.clone(),
                    container_id: container_id.to_string(),
                },
            )
            .filter(|previous| previous.container_id != container_id);
        previous.map(|previous| previous.container_id)
    }

    /// Record an environment that has no container behind it yet.
    pub fn record(&self, target: &ConvergeTarget) {
        self.replace(target, "");
    }

    /// Look up a previously converged release by id. A release this
    /// provider never converged is *not* a rollback target.
    pub fn by_release(&self, release_id: Uuid) -> Option<ConvergeTarget> {
        self.converged.lock().ok().and_then(|guard| {
            guard
                .values()
                .find(|converged| converged.target.release_id == release_id)
                .map(|converged| converged.target.clone())
        })
    }
}

/// Resolves an opaque deployment-plan target onto a host target.
/// The plan carries only a `forge:<slug>:<environment>` identity and
/// a digest, so the composition that owns the delivery state
/// implements this lookup instead of the adapter guessing.
pub trait PlanTargetResolver: Send + Sync + 'static {
    /// Resolve a plan target + requested digest. `Ok(None)` means the
    /// target is unknown to this process.
    fn resolve(
        &self,
        plan_target: &str,
        release_digest: &str,
    ) -> Result<Option<ConvergeTarget>, ApplicationDeliveryError>;
}

/// Target scheme the forge protocol publishes in a plan.
const FORGE_TARGET_PREFIX: &str = "forge:";

/// [`PlanTargetResolver`] over the live delivery state. The forge
/// facade's plan identity is `forge:<project-slug>:<environment>`, so
/// the resolver projects the slug onto the application whose name is
/// the slug tail — the same rule the facade uses to find the
/// application — and then projects the newest release of that
/// environment onto a host target.
pub struct DeliveryTargetResolver {
    delivery: Arc<super::ApplicationDeliveryService>,
    caller: User,
}

impl DeliveryTargetResolver {
    /// Compose the resolver for one authorised caller.
    pub fn new(delivery: Arc<super::ApplicationDeliveryService>, caller: User) -> Self {
        Self { delivery, caller }
    }
}

impl PlanTargetResolver for DeliveryTargetResolver {
    fn resolve(
        &self,
        plan_target: &str,
        _release_digest: &str,
    ) -> Result<Option<ConvergeTarget>, ApplicationDeliveryError> {
        let identity = plan_target
            .strip_prefix(FORGE_TARGET_PREFIX)
            .ok_or_else(|| {
                ApplicationDeliveryError::Invalid("plan target is not a forge target".into())
            })?;
        let (slug, environment_name) = identity.rsplit_once(':').ok_or_else(|| {
            ApplicationDeliveryError::Invalid("plan target has no environment".into())
        })?;
        let needle = slug.rsplit('/').next().unwrap_or(slug);
        let application = self
            .delivery
            .list_applications(&self.caller)
            .map_err(|error| ApplicationDeliveryError::Invalid(error.to_string()))?
            .into_iter()
            .find(|application| application.name() == needle)
            .ok_or_else(|| {
                ApplicationDeliveryError::Invalid("application is not registered".into())
            })?;
        let environment = application
            .environment(environment_name)
            .ok_or(ApplicationDeliveryError::MissingEnvironment(
                "environment not found",
            ))?
            .clone();
        let releases = self
            .delivery
            .list_releases(&self.caller, application.id())
            .map_err(|error| ApplicationDeliveryError::Invalid(error.to_string()))?;
        let release = releases
            .iter()
            .filter(|release| release.environment_id() == environment.id())
            .max_by_key(|release| release.created_at())
            .ok_or(ApplicationDeliveryError::ReleaseNotReady(
                "no release for environment",
            ))?;
        ConvergeTarget::new(
            application.id(),
            environment.id(),
            application.name(),
            environment.name(),
            release.id(),
            release.digest().value(),
        )
        .map(Some)
    }
}

/// The outcome of one converge, plus the container that backs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvergeResult {
    /// Terminal delivery outcome for the release.
    pub outcome: ReleaseOutcome,
    /// Container backing the environment, or `None` when the converge
    /// failed and the container was discarded.
    pub container_id: Option<String>,
}

/// Production runtime adapter. Converges an environment onto a
/// pinned release through the existing docker service.
pub struct DockerRuntimeAdapter {
    docker: Arc<dyn DockerAdapter>,
    network: Arc<dyn NetworkAdapter>,
    namespace: String,
    registry: Arc<ConvergeRegistry>,
    health_polls: u32,
}

impl DockerRuntimeAdapter {
    /// Compose the adapter. `namespace` is the stable remote
    /// registry namespace the build adapter pushes into, so a
    /// digest-only release resolves to the same image after a
    /// process restart.
    pub fn new(
        docker: Arc<dyn DockerAdapter>,
        network: Arc<dyn NetworkAdapter>,
        namespace: impl Into<String>,
        registry: Arc<ConvergeRegistry>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let namespace = namespace.into();
        if namespace.trim().is_empty()
            || namespace.contains('@')
            || namespace.chars().any(char::is_whitespace)
        {
            return Err(ApplicationDeliveryError::Invalid(
                "delivery registry namespace is invalid".into(),
            ));
        }
        Ok(Self {
            docker,
            network,
            namespace: namespace.trim_end_matches('/').into(),
            registry,
            health_polls: DEFAULT_HEALTH_POLLS,
        })
    }

    /// Shared projection both adapters agree on.
    pub fn registry(&self) -> Arc<ConvergeRegistry> {
        self.registry.clone()
    }

    /// Read-only converge check. Pulls nothing, creates nothing: it
    /// asserts the daemon is reachable and the pinned image is
    /// already present, which is exactly what a dry-run may do.
    pub fn check(
        &self,
        target: &ConvergeTarget,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        let image = target.image_reference(&self.namespace);
        let docker = self.docker.clone();
        let reference = image.clone();
        let present = run_docker(async move {
            docker.ping().await?;
            docker.image_exists(&reference).await
        })
        .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
        if !present {
            return Ok(failed(format!(
                "release image is not present on the host: {}",
                bounded(&image)
            )));
        }
        Ok(healthy(format!(
            "converge check passed for {}",
            bounded(&image)
        )))
    }

    /// Converge an environment onto the target release. Mutates the
    /// host: ensures the image, creates the labeled container, starts
    /// it, waits for a terminal engine state, then retires the
    /// container that backed the previous release.
    pub async fn converge(
        &self,
        target: &ConvergeTarget,
    ) -> Result<ConvergeResult, ApplicationDeliveryError> {
        converge_and_record(
            self.docker.clone(),
            self.network.clone(),
            &self.namespace,
            self.health_polls,
            target,
            &self.registry,
        )
        .await
    }

    /// Synchronous converge for the domain ports. The engine I/O
    /// needs a Tokio runtime, so it runs on a dedicated worker
    /// thread; a provider frame already executes inside Tokio and a
    /// nested `block_on` would panic.
    pub fn converge_blocking(
        &self,
        target: &ConvergeTarget,
    ) -> Result<ConvergeResult, ApplicationDeliveryError> {
        let docker = self.docker.clone();
        let network = self.network.clone();
        let namespace = self.namespace.clone();
        let health_polls = self.health_polls;
        let registry = self.registry.clone();
        let target = target.clone();
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let worker = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
                    worker.block_on(converge_and_record(
                        docker,
                        network,
                        &namespace,
                        health_polls,
                        &target,
                        &registry,
                    ))
                })
                .join()
                .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?
        })
    }
}

impl RuntimeAdapter for DockerRuntimeAdapter {
    fn adapter_id(&self) -> &'static str {
        "oci-docker-runtime-v1"
    }

    fn deploy(
        &self,
        release: &Release,
        environment: &Environment,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        let target = self.target_for(release, environment)?;
        Ok(self.converge_blocking(&target)?.outcome)
    }

    fn rollback(
        &self,
        release: &Release,
        environment: &Environment,
        target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        // The current release must still belong to this environment:
        // a rollback is only ever scoped to the environment that is
        // actually misbehaving.
        let current = self.target_for(release, environment)?;
        let target = self.registry.by_release(target_release_id).ok_or_else(|| {
            ApplicationDeliveryError::IllegalTransition(
                "rollback target release was never converged by this provider".into(),
            )
        })?;
        if target.environment_id() != current.environment_id() {
            return Err(ApplicationDeliveryError::IllegalTransition(
                "rollback target release belongs to another environment".into(),
            ));
        }
        Ok(self.converge_blocking(&target)?.outcome)
    }
}

impl DockerRuntimeAdapter {
    /// Bind a domain release to a rollout target.
    fn target_for(
        &self,
        release: &Release,
        environment: &Environment,
    ) -> Result<ConvergeTarget, ApplicationDeliveryError> {
        if release.environment_id() != environment.id() {
            return Err(ApplicationDeliveryError::IllegalTransition(
                "release does not belong to the target environment".into(),
            ));
        }
        ConvergeTarget::from_release(release, environment)
    }
}

/// Converge, then record the container that backs the environment and
/// retire the one it replaced. Both the synchronous domain port and
/// the async deployment adapter go through here, so a release loop
/// cannot accumulate containers on the host and a later rollback can
/// find the release it needs.
async fn converge_and_record(
    docker: Arc<dyn DockerAdapter>,
    network: Arc<dyn NetworkAdapter>,
    namespace: &str,
    health_polls: u32,
    target: &ConvergeTarget,
    registry: &ConvergeRegistry,
) -> Result<ConvergeResult, ApplicationDeliveryError> {
    let converged = converge_with(docker.clone(), network, namespace, health_polls, target).await?;
    if let Some(container_id) = converged.container_id.clone()
        && let Some(retired) = registry.replace(target, &container_id)
    {
        let _ = run_docker(async move { docker.remove(&retired, true).await });
    }
    Ok(converged)
}

/// The converge itself, factored out so the synchronous domain port
/// and the async deployment adapter share one implementation and one
/// health policy.
async fn converge_with(
    docker: Arc<dyn DockerAdapter>,
    network: Arc<dyn NetworkAdapter>,
    namespace: &str,
    health_polls: u32,
    target: &ConvergeTarget,
) -> Result<ConvergeResult, ApplicationDeliveryError> {
    let image = target.image_reference(namespace);
    ensure_image(&docker, &image, target.digest()).map_err(map_docker_error)?;
    let spec = target.container_spec(image);
    // No port range is granted: a delivery rollout publishes no host
    // port, so every binding would be out of range.
    spec.validate(0..=0).map_err(|_| {
        ApplicationDeliveryError::Invalid("delivery container spec is invalid".into())
    })?;
    network
        .ensure(None)
        .await
        .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
    let created = {
        let runtime = docker.clone();
        let spec = spec.clone();
        let labels = target.labels();
        run_docker(async move { runtime.create_labeled(&spec, &labels).await })
    }
    .map_err(map_docker_error)?;
    let started = {
        let runtime = docker.clone();
        let id = created.clone();
        run_docker(async move { runtime.start(&id).await })
    };
    if let Err(error) = started {
        // Never leave a created-but-unusable container behind.
        discard(&docker, &created).await;
        return Err(map_docker_error(error));
    }
    let outcome = await_terminal_state(&docker, &created, health_polls).await?;
    if outcome.state != ReleaseState::Healthy {
        // An unhealthy converge is discarded rather than left
        // half-deployed, and nothing is recorded for it.
        discard(&docker, &created).await;
        return Ok(ConvergeResult {
            outcome,
            container_id: None,
        });
    }
    Ok(ConvergeResult {
        outcome,
        container_id: Some(created),
    })
}

/// Remove a container this adapter created but cannot use.
async fn discard(docker: &Arc<dyn DockerAdapter>, id: &str) {
    let runtime = docker.clone();
    let id = id.to_string();
    let _ = run_docker(async move { runtime.remove(&id, true).await });
}

/// Poll the container until the engine reports a terminal state. The
/// policy honors an image-declared HEALTHCHECK verdict and otherwise
/// accepts a running container, so an image without a health check
/// is not punished for having none.
async fn await_terminal_state(
    docker: &Arc<dyn DockerAdapter>,
    id: &str,
    health_polls: u32,
) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
    let mut outcome = failed("container never reported a terminal state".into());
    for attempt in 0..health_polls.max(1) {
        if attempt > 0 {
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
        let runtime = docker.clone();
        let reference = id.to_string();
        let state: RuntimeContainerState =
            run_docker(async move { runtime.inspect(&reference).await })
                .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
        outcome = project_state(&state);
        if outcome.state != ReleaseState::Healthy || state.health.as_deref() == Some("healthy") {
            return Ok(outcome);
        }
    }
    Ok(outcome)
}

/// Make the pinned image present in the engine, pulling it only when
/// it is missing. The pull result is compared against the requested
/// digest so a retagged upstream can never back a release.
fn ensure_image(
    docker: &Arc<dyn DockerAdapter>,
    image: &str,
    digest: &str,
) -> Result<(), DockerError> {
    let runtime = docker.clone();
    let reference = image.to_string();
    let present = run_docker(async move { runtime.image_exists(&reference).await })?;
    if present {
        return Ok(());
    }
    let runtime = docker.clone();
    let reference = image.to_string();
    let pulled = run_docker(async move { runtime.pull(&reference).await })?;
    if !pulled.ends_with(digest) {
        return Err(DockerError::Invalid(
            "registry returned a different digest".into(),
        ));
    }
    Ok(())
}

/// Project the engine state onto a terminal outcome. Bounded and
/// payload-free: no environment variable, no credential, no raw log.
fn project_state(state: &RuntimeContainerState) -> ReleaseOutcome {
    if state.oom_killed {
        return failed("container was OOM killed".into());
    }
    match state.health.as_deref() {
        Some("healthy") => return healthy("container reported healthy".into()),
        Some("unhealthy") => return failed("container reported unhealthy".into()),
        Some(_) | None => {}
    }
    if state.status == "running" {
        healthy("container is running".into())
    } else {
        failed(format!("container status is {}", bounded(&state.status)))
    }
}

fn healthy(diagnostic: String) -> ReleaseOutcome {
    ReleaseOutcome {
        state: ReleaseState::Healthy,
        diagnostic: bounded_diagnostic(&diagnostic),
    }
}

fn failed(diagnostic: String) -> ReleaseOutcome {
    ReleaseOutcome {
        state: ReleaseState::Failed,
        diagnostic: bounded_diagnostic(&diagnostic),
    }
}

fn bounded_diagnostic(value: &str) -> String {
    let redacted = redact_diagnostic(value);
    if redacted.len() <= MAX_DIAGNOSTIC_LEN {
        return redacted;
    }
    redacted.chars().take(MAX_DIAGNOSTIC_LEN).collect()
}

fn bounded(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || ".:-_@/".contains(*c))
        .take(MAX_DIAGNOSTIC_LEN)
        .collect()
}

fn container_name(environment_id: Uuid, release_id: Uuid) -> String {
    let environment = &environment_id.simple().to_string()[..8];
    let release = &release_id.simple().to_string()[..8];
    format!("openpanel-{environment}-{release}")
}

fn is_immutable_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Map a docker failure onto the typed delivery error. The upstream
/// text is dropped: a socket path, a registry URL, or a credential
/// must never reach an audit row.
fn map_docker_error(error: DockerError) -> ApplicationDeliveryError {
    match error {
        DockerError::Forbidden => ApplicationDeliveryError::RegistryUnauthorized,
        _ => ApplicationDeliveryError::RuntimeUnavailable,
    }
}

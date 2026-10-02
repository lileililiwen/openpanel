//! Production deployment adapter over the docker delivery runtime.
//!
//! [`DockerDeploymentAdapter`] is the `DeploymentAdapter`
//! implementation the forge publish provider composes. It owns the
//! plan-side contract — manifest identity, plan/manifest refusal,
//! dry-run, and idempotent replay — and delegates the host work to
//! [`DockerRuntimeAdapter`], so there is exactly one converge
//! implementation in the product.

use std::sync::Arc;

use chrono::Utc;
use openpanel_domain::deployment_adapters::{
    AdapterManifest, DeploymentAction, DeploymentAdapter, DeploymentEvidence, DeploymentPlan,
    DeploymentState, HealthMethod, IdempotencyDecision, RUNTIME_CONTRACT_VERSION, decide_replay,
    validate_plan,
};

use crate::application_delivery::{ConvergeTarget, DockerRuntimeAdapter, PlanTargetResolver};

/// Stable adapter id. The forge provider composes plans against this
/// id, so renaming it is a protocol-visible change.
pub const DOCKER_DEPLOYMENT_ADAPTER_ID: &str = "docker-deploy-v1";

/// Target scheme the adapter declares. Plans carry the forge target
/// identity `forge:<project-slug>:<environment>`.
const TARGET_SCHEME: &str = "forge:";

/// Production deployment adapter.
pub struct DockerDeploymentAdapter {
    manifest: AdapterManifest,
    runtime: Arc<DockerRuntimeAdapter>,
    resolver: Arc<dyn PlanTargetResolver>,
    unreportable: DeploymentEvidence,
}

impl DockerDeploymentAdapter {
    /// Compose the adapter from the shared runtime adapter and the
    /// composition-owned plan-target resolver.
    pub fn new(
        runtime: Arc<DockerRuntimeAdapter>,
        resolver: Arc<dyn PlanTargetResolver>,
    ) -> Result<Self, openpanel_domain::DeploymentAdapterError> {
        let manifest = AdapterManifest::new(
            DOCKER_DEPLOYMENT_ADAPTER_ID,
            "OpenPanel Docker Delivery",
            RUNTIME_CONTRACT_VERSION,
            TARGET_SCHEME,
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Rollback,
            ],
            true,
            // The runtime adapter's own health policy is the
            // post-apply check; the manifest only has to declare one.
            HealthMethod::Command {
                argv: vec!["openpanel-health".into()],
            },
            // The rollout resolves no secret value: the container
            // spec carries no environment variable, so the adapter
            // declares nothing until a composition tells it which
            // secret-set names a plan may reference.
            Vec::new(),
        )?;
        Ok(Self {
            manifest,
            runtime,
            resolver,
            unreportable: unreportable_evidence()?,
        })
    }

    /// Declare the secret-reference names this composition accepts.
    ///
    /// [`validate_plan`] refuses a plan whose secret references are
    /// not declared by the manifest, and the names are chosen per
    /// panel (they are the operator's environment secret sets), not
    /// by the adapter. A composition therefore starts with nothing
    /// declared and names what it accepts here.
    pub fn with_declared_secret_refs(
        self,
        names: Vec<String>,
    ) -> Result<Self, openpanel_domain::DeploymentAdapterError> {
        let manifest = AdapterManifest::new(
            self.manifest.id().to_string(),
            self.manifest.label().to_string(),
            self.manifest.contract_version(),
            self.manifest.target_scheme().to_string(),
            self.manifest.actions().to_vec(),
            self.manifest.supports_rollback(),
            self.manifest.health().clone(),
            names,
        )?;
        Ok(Self { manifest, ..self })
    }
}

impl DeploymentAdapter for DockerDeploymentAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn execute(
        &self,
        plan: &DeploymentPlan,
        cached: Option<&DeploymentEvidence>,
    ) -> DeploymentEvidence {
        let started_at = Utc::now();
        // A foreign plan never touches the host: the refusal happens
        // before any resolve or converge call.
        if let Err(error) = validate_plan(plan, &self.manifest) {
            return terminal(
                plan,
                DeploymentState::PreflightRejected,
                started_at,
                error.to_string(),
                &self.unreportable,
            );
        }
        // Idempotent replay returns the cached record verbatim and
        // performs no host operation.
        if decide_replay(plan, cached) == IdempotencyDecision::Replay
            && let Some(cached) = cached
        {
            return cached.clone();
        }
        let target = match self.resolve(plan) {
            Ok(target) => target,
            Err(diagnostic) => {
                return terminal(
                    plan,
                    refused_state(plan),
                    started_at,
                    diagnostic,
                    &self.unreportable,
                );
            }
        };
        if plan.dry_run() || !plan.action().is_mutating() {
            // Dry-run and the read-only actions run the full
            // read-only converge check: the docker call log records
            // zero mutations.
            return match self.runtime.check(&target) {
                Ok(outcome) => terminal(
                    plan,
                    DeploymentState::Ready,
                    started_at,
                    outcome.diagnostic,
                    &self.unreportable,
                ),
                Err(error) => terminal(
                    plan,
                    DeploymentState::Failed,
                    started_at,
                    error.to_string(),
                    &self.unreportable,
                ),
            };
        }
        match self.runtime.converge_blocking(&target) {
            Ok(converged) => terminal(
                plan,
                DeploymentState::Ready,
                started_at,
                converged.outcome.diagnostic,
                &self.unreportable,
            ),
            Err(error) => terminal(
                plan,
                DeploymentState::Failed,
                started_at,
                error.to_string(),
                &self.unreportable,
            ),
        }
    }
}

impl DockerDeploymentAdapter {
    /// Resolve the plan's opaque target onto a host target.
    fn resolve(&self, plan: &DeploymentPlan) -> Result<ConvergeTarget, String> {
        if !plan.target().starts_with(TARGET_SCHEME) {
            return Err(format!(
                "plan target `{}` is outside the {} namespace",
                bounded(plan.target()),
                TARGET_SCHEME
            ));
        }
        match self.resolver.resolve(plan.target(), plan.release_digest()) {
            Ok(Some(target)) => Ok(target),
            Ok(None) => Err(format!(
                "plan target `{}` has no converged release in this process",
                bounded(plan.target())
            )),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// A preflight that cannot resolve its target is rejected; a deploy
/// that cannot resolve its target failed. Both are terminal.
fn refused_state(plan: &DeploymentPlan) -> DeploymentState {
    match plan.action() {
        DeploymentAction::Preflight => DeploymentState::PreflightRejected,
        _ => DeploymentState::Failed,
    }
}

fn terminal(
    plan: &DeploymentPlan,
    state: DeploymentState,
    started_at: chrono::DateTime<Utc>,
    diagnostic: String,
    unreportable: &DeploymentEvidence,
) -> DeploymentEvidence {
    // Every field the plan carried already passed `DeploymentPlan::new`
    // validation, so this construction succeeds; the `Err` arm keeps
    // the trait total without panicking.
    match DeploymentEvidence::new(
        plan.target(),
        plan.adapter(),
        plan.action(),
        plan.operation_key().cloned(),
        plan.release_digest(),
        state,
        started_at,
        started_at,
        diagnostic,
    ) {
        Ok(evidence) => evidence,
        Err(_) => unreportable.clone(),
    }
}

fn bounded(value: &str) -> String {
    value.chars().take(254).collect()
}

/// The record returned when a plan's own fields cannot be projected
/// into an evidence record. Built once at composition time from
/// known-good literals, so a composition that cannot build it fails
/// loudly in [`DockerDeploymentAdapter::new`] instead of panicking
/// later on a provider frame.
fn unreportable_evidence() -> Result<DeploymentEvidence, openpanel_domain::DeploymentAdapterError> {
    let now = Utc::now();
    DeploymentEvidence::new(
        "docker-deploy",
        DOCKER_DEPLOYMENT_ADAPTER_ID,
        DeploymentAction::Status,
        None,
        "sha256:0",
        DeploymentState::Failed,
        now,
        now,
        "plan evidence could not be recorded",
    )
}

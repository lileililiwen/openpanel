//! Deployment-adapter pure logic: validate a plan against a
//! manifest, derive a stable idempotency record id, decide
//! whether a replay is allowed, and the [`DeploymentAdapter`]
//! trait every concrete adapter must implement. The data
//! types live in `types.rs`; the unit + property tests live
//! in `tests.rs`. This file is I/O-free.

use crate::deployment_adapters::types::{
    AdapterManifest, DeploymentAction, DeploymentAdapterError, DeploymentEvidence, DeploymentPlan,
    RollbackPolicy,
};
use uuid::Uuid;

/// Pure decision function: validate a plan against a manifest
/// before the adapter is allowed to run it.
///
/// Returns the [`RollbackPolicy`] the adapter should apply if the
/// plan fails. Refuses the plan when the manifest does not
/// declare the action, when the secret-reference set is a
/// superset of the manifest's declared names, or when a
/// mutating action arrives without an `OperationKey`.
pub fn validate_plan(
    plan: &DeploymentPlan,
    manifest: &AdapterManifest,
) -> Result<RollbackPolicy, DeploymentAdapterError> {
    if plan.adapter() != manifest.id() {
        return Err(DeploymentAdapterError::Invalid(format!(
            "plan targets adapter `{}` but manifest is `{}`",
            plan.adapter(),
            manifest.id()
        )));
    }
    if !manifest.supports(plan.action()) {
        return Err(DeploymentAdapterError::UnsupportedAction(
            plan.action().as_str(),
        ));
    }
    if plan.action().is_mutating() && plan.operation_key().is_none() {
        return Err(DeploymentAdapterError::Invalid(
            "mutating action requires an operation key".into(),
        ));
    }
    for ref_name in plan.secret_refs() {
        if !manifest
            .secret_refs()
            .iter()
            .any(|declared| declared == ref_name.name())
        {
            return Err(DeploymentAdapterError::Invalid(format!(
                "plan references undeclared secret `{}`",
                ref_name.name()
            )));
        }
    }
    if matches!(plan.action(), DeploymentAction::Rollback) && !manifest.supports_rollback() {
        return Err(DeploymentAdapterError::Invalid(
            "adapter does not declare rollback support".into(),
        ));
    }
    Ok(plan.action().rollback_policy())
}

/// Stable id derivation for an idempotency record so the
/// application layer can dedupe replays deterministically.
pub fn operation_record_id(plan: &DeploymentPlan) -> Result<Uuid, DeploymentAdapterError> {
    let key = plan
        .operation_key()
        .ok_or_else(|| DeploymentAdapterError::Invalid("operation key required".into()))?;
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(plan.adapter().as_bytes());
    hasher.update([0]);
    hasher.update(key.target().as_bytes());
    hasher.update([0]);
    hasher.update(key.action().as_bytes());
    hasher.update([0]);
    hasher.update(key.value().as_bytes());
    hasher.update([0]);
    hasher.update(plan.release_digest().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(Uuid::from_bytes(bytes))
}

/// Pure decision: should a replay of `plan` return the cached
/// evidence from a prior run, or run a fresh attempt? Returns
/// `Replay` when the cached evidence exists, is in a terminal
/// state, and the release digest matches; `Rerun` otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyDecision {
    /// Return the cached terminal evidence.
    Replay,
    /// Run a fresh attempt.
    Rerun,
}

/// Decide whether a new plan should be a `Replay` (return the
/// cached terminal evidence) or a `Rerun` (invoke the adapter
/// again). A replay is granted only when:
/// 1. a cached terminal evidence record exists for the same
///    `(target, action, release_digest)`,
/// 2. the cached state is terminal, AND
/// 3. the new plan is not a dry-run.
///
/// A dry-run is always a `Rerun` so the caller can preview
/// the action rather than replay a previous result.
pub fn decide_replay(
    plan: &DeploymentPlan,
    cached: Option<&DeploymentEvidence>,
) -> IdempotencyDecision {
    let Some(cached) = cached else {
        return IdempotencyDecision::Rerun;
    };
    if !cached.state().is_terminal() {
        return IdempotencyDecision::Rerun;
    }
    if cached.release_digest() != plan.release_digest() {
        return IdempotencyDecision::Rerun;
    }
    if cached.action() != plan.action() {
        return IdempotencyDecision::Rerun;
    }
    if plan.dry_run() {
        return IdempotencyDecision::Rerun;
    }
    IdempotencyDecision::Replay
}

/// The contract every deployment adapter must implement. The app
/// layer provides one or more concrete adapters (OCI/Compose,
/// generic systemd, the Mac/Jenkins conformance fixture, etc.).
pub trait DeploymentAdapter: Send + Sync + 'static {
    /// The manifest the adapter publishes. The app service
    /// refuses plans whose `adapter` field does not match this
    /// id.
    fn manifest(&self) -> &AdapterManifest;

    /// Run a plan. The implementation MUST:
    /// - validate the plan against its own manifest before
    ///   mutating,
    /// - respect the dry-run flag,
    /// - honour the idempotency record returned by the app
    ///   service on replay,
    /// - return a `DeploymentEvidence` in a terminal state.
    fn execute(
        &self,
        plan: &DeploymentPlan,
        cached: Option<&DeploymentEvidence>,
    ) -> DeploymentEvidence;
}

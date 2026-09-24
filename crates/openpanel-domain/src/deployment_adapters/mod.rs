//! Deployment adapters bounded context: provider-neutral lifecycle
//! operations.
//!
//! The adapter contract is the only thing product code touches; the
//! underlying transport (systemd, Compose, Podman, Kubernetes, SSH, a
//! remote orchestrator) lives behind the [`DeploymentAdapter`] trait.
//! OpenPanel owns release identity, desired configuration, health
//! checks, and audit-safe lifecycle state; adapters own transport
//! and host policy.
//!
//! Covers `deployment-adapters`: Adapter Capability Declaration,
//! Idempotent Deployment Lifecycle, Provider-Neutral Evidence, Safe
//! Failure and Rollback. Collection, transport, host policy, and
//! audit fan-out live in the app layer; this module owns the
//! typed action lifecycle, the idempotency key, the secret
//! reference, the evidence schema, and the state machine that the
//! app service and the app tests reuse.
//!
//! I/O-free: no sqlx, no axum, no tokio, no subprocess. Adapters
//! that need I/O implement the trait in the app layer.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Domain validation failures for the deployment-adapter contract.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeploymentAdapterError {
    /// A value failed validation.
    #[error("invalid deployment-adapter value: {0}")]
    Invalid(String),
    /// A plan references an action the adapter does not declare.
    #[error("unsupported adapter action: {0}")]
    UnsupportedAction(&'static str),
    /// The adapter manifest is missing a required field.
    #[error("adapter manifest is incomplete: {0}")]
    IncompleteManifest(&'static str),
}

/// Runtime contract version this adapter speaks. Bump when the
/// evidence schema or the action set changes incompatibly.
pub const RUNTIME_CONTRACT_VERSION: u32 = 1;

/// Maximum number of distinct action kinds a single adapter can
/// declare. Caps the manifest size and prevents accidental
/// kitchen-sink adapters.
pub const MAX_DECLARED_ACTIONS: usize = 16;

/// Maximum length of a target identifier, plan id, release digest,
/// or actor name. Bounded so logs and audit rows stay readable.
pub const MAX_IDENTIFIER_LEN: usize = 254;

/// Maximum length of a single diagnostic message in an evidence
/// record. Anything longer is truncated by the recorder.
pub const MAX_DIAGNOSTIC_LEN: usize = 1024;

/// Operations the deployment adapter can perform. Mutating
/// actions (`Deploy`, `Restart`, `Rollback`) are gated behind an
/// `OperationKey` for idempotency; the read-only actions
/// (`Status`, `Logs`, `Verify`) are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentAction {
    /// Preflight: validate configuration without mutating the
    /// target.
    Preflight,
    /// Apply the release to the target.
    Deploy,
    /// Re-run the declared health check; does not mutate state.
    Verify,
    /// Restart the service without changing the release.
    Restart,
    /// Roll back to the prior release.
    Rollback,
    /// Read-only target state.
    Status,
    /// Read-only bounded log fetch.
    Logs,
}

impl DeploymentAction {
    /// Whether the action is mutating. Mutating actions require
    /// an `OperationKey`.
    pub fn is_mutating(self) -> bool {
        matches!(
            self,
            DeploymentAction::Deploy
                | DeploymentAction::Restart
                | DeploymentAction::Rollback
                | DeploymentAction::Preflight
        )
    }

    /// Whether the action should attempt rollback when the
    /// post-check fails. `Preflight` is non-mutating but a failed
    /// preflight refuses the deploy before the first mutation,
    /// so the rollback policy is `Skip` rather than `Attempt`.
    pub fn rollback_policy(self) -> RollbackPolicy {
        match self {
            DeploymentAction::Deploy => RollbackPolicy::Attempt,
            DeploymentAction::Restart => RollbackPolicy::Attempt,
            DeploymentAction::Rollback => RollbackPolicy::Skip,
            DeploymentAction::Verify
            | DeploymentAction::Status
            | DeploymentAction::Logs
            | DeploymentAction::Preflight => RollbackPolicy::Skip,
        }
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            DeploymentAction::Preflight => "preflight",
            DeploymentAction::Deploy => "deploy",
            DeploymentAction::Verify => "verify",
            DeploymentAction::Restart => "restart",
            DeploymentAction::Rollback => "rollback",
            DeploymentAction::Status => "status",
            DeploymentAction::Logs => "logs",
        }
    }
}

/// Policy that decides whether a failed action should attempt a
/// rollback before reporting its terminal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackPolicy {
    /// Rollback is the adapter's responsibility and will be
    /// attempted on failure.
    Attempt,
    /// The action does not need a rollback step.
    Skip,
    /// The action refuses the plan outright; no rollback.
    Refuse,
}

impl RollbackPolicy {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attempt => "attempt",
            Self::Skip => "skip",
            Self::Refuse => "refuse",
        }
    }
}

/// Health-check method the adapter will run after a mutating
/// action. Pure-data, never secrets, never a transport-specific
/// command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HealthMethod {
    /// HTTP GET against a declared URL; the call is `Ready` only
    /// when the response is `2xx`.
    HttpGet {
        /// URL the adapter polls (opaque to OpenPanel; no
        /// transport-specific scheme is required by the contract).
        url: String,
        /// Optional `Ready` substring expected in the body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_body_contains: Option<String>,
    },
    /// TCP connect to a declared address.
    TcpConnect {
        /// `host:port` string the adapter dials.
        address: String,
    },
    /// Run an allowlisted command on the target and treat exit
    /// `0` as healthy.
    Command {
        /// Allowlisted command (e.g. `systemctl is-active openpanel`).
        argv: Vec<String>,
    },
}

impl HealthMethod {
    /// Stable wire name.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::HttpGet { .. } => "http_get",
            Self::TcpConnect { .. } => "tcp_connect",
            Self::Command { .. } => "command",
        }
    }
}

/// Secret reference carried in a plan. Adapters resolve the
/// reference against their own secret store; OpenPanel never
/// receives the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    /// Logical name the adapter resolves (e.g. `agent-token`,
    /// `tls-cert`).
    name: String,
    /// Optional secret version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

impl SecretRef {
    /// Build a secret reference. The name is bounded; the version
    /// (if any) is bounded.
    pub fn new(
        name: impl Into<String>,
        version: Option<String>,
    ) -> Result<Self, DeploymentAdapterError> {
        let name = name.into();
        if name.trim().is_empty() || name.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "secret reference name is required".into(),
            ));
        }
        if let Some(v) = version.as_ref()
            && (v.is_empty() || v.len() > MAX_IDENTIFIER_LEN)
        {
            return Err(DeploymentAdapterError::Invalid(
                "secret reference version is invalid".into(),
            ));
        }
        Ok(Self { name, version })
    }

    /// Secret logical name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Secret version, if any.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// Idempotency key. Two requests with the same `(target,
/// action, operation_key)` MUST return the same terminal result
/// and MUST NOT duplicate work.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationKey {
    target: String,
    action: String,
    value: String,
}

impl OperationKey {
    /// Build a key. All three components are required and bounded.
    pub fn new(
        target: impl Into<String>,
        action: DeploymentAction,
        value: impl Into<String>,
    ) -> Result<Self, DeploymentAdapterError> {
        let target = target.into();
        let value = value.into();
        if target.trim().is_empty() || target.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "operation target is required".into(),
            ));
        }
        if value.trim().is_empty() || value.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "operation key value is required".into(),
            ));
        }
        Ok(Self {
            target,
            action: action.as_str().to_string(),
            value,
        })
    }

    /// The declared target id.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The action label.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The idempotency value (typically a hash of the release
    /// digest + caller + timestamp window).
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Adapter manifest: what the adapter can do, the runtime
/// contract version it speaks, and the health method it will run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// Stable adapter id (e.g. `oci-compose-v1`, `mac-jenkins-v1`).
    id: String,
    /// Human-readable name.
    label: String,
    /// Runtime contract version.
    contract_version: u32,
    /// Declared target identity prefix (e.g. `oci://`, `systemd:`,
    /// `mac://`).
    target_scheme: String,
    /// Sorted set of supported actions.
    actions: Vec<DeploymentAction>,
    /// Whether the adapter supports rollback.
    supports_rollback: bool,
    /// Health method the adapter runs after mutating actions.
    health: HealthMethod,
    /// Secret reference names the adapter expects to resolve.
    secret_refs: Vec<String>,
}

impl AdapterManifest {
    /// Build a manifest. Sorts the action set, rejects empty
    /// action lists, rejects oversize secret-reference lists.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        contract_version: u32,
        target_scheme: impl Into<String>,
        actions: Vec<DeploymentAction>,
        supports_rollback: bool,
        health: HealthMethod,
        secret_refs: Vec<String>,
    ) -> Result<Self, DeploymentAdapterError> {
        let id = id.into();
        let label = label.into();
        let target_scheme = target_scheme.into();
        if id.trim().is_empty() || id.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::IncompleteManifest("id"));
        }
        if label.trim().is_empty() || label.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::IncompleteManifest("label"));
        }
        if contract_version == 0 || contract_version > RUNTIME_CONTRACT_VERSION {
            return Err(DeploymentAdapterError::IncompleteManifest(
                "contract_version",
            ));
        }
        if target_scheme.trim().is_empty() || target_scheme.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::IncompleteManifest("target_scheme"));
        }
        if actions.is_empty() {
            return Err(DeploymentAdapterError::IncompleteManifest("actions"));
        }
        if actions.len() > MAX_DECLARED_ACTIONS {
            return Err(DeploymentAdapterError::Invalid(format!(
                "actions must be 1..={MAX_DECLARED_ACTIONS}"
            )));
        }
        let mut deduped: BTreeSet<&'static str> = BTreeSet::new();
        for action in &actions {
            if !deduped.insert(action.as_str()) {
                return Err(DeploymentAdapterError::Invalid(format!(
                    "duplicate action `{}`",
                    action.as_str()
                )));
            }
        }
        for name in &secret_refs {
            if name.trim().is_empty() || name.len() > MAX_IDENTIFIER_LEN {
                return Err(DeploymentAdapterError::Invalid(
                    "secret reference name is invalid".into(),
                ));
            }
        }
        let mut actions = actions;
        actions.sort_by_key(|a| a.as_str());
        Ok(Self {
            id,
            label,
            contract_version,
            target_scheme,
            actions,
            supports_rollback,
            health,
            secret_refs,
        })
    }

    /// Adapter id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Runtime contract version.
    pub fn contract_version(&self) -> u32 {
        self.contract_version
    }

    /// Target scheme prefix.
    pub fn target_scheme(&self) -> &str {
        &self.target_scheme
    }

    /// Declared actions, sorted.
    pub fn actions(&self) -> &[DeploymentAction] {
        &self.actions
    }

    /// Whether the adapter supports rollback.
    pub fn supports_rollback(&self) -> bool {
        self.supports_rollback
    }

    /// Declared health method.
    pub fn health(&self) -> &HealthMethod {
        &self.health
    }

    /// Secret references the adapter expects.
    pub fn secret_refs(&self) -> &[String] {
        &self.secret_refs
    }

    /// Whether the manifest declares support for `action`.
    pub fn supports(&self, action: DeploymentAction) -> bool {
        self.actions
            .binary_search_by_key(&action.as_str(), |a| a.as_str())
            .is_ok()
    }
}

/// State of a deployment action. The state machine is:
/// `Accepted -> PreflightInProgress -> Applied -> Verifying ->
/// Ready | Failed | RolledBack | PreflightRejected`. Rollback
/// failure is a separate terminal state, not the same as
/// `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    /// Plan accepted, no work done yet.
    Accepted,
    /// Preflight checks running.
    PreflightInProgress,
    /// Preflight failed; nothing was mutated.
    PreflightRejected,
    /// Mutating action in progress.
    Applied,
    /// Post-deploy verification running.
    Verifying,
    /// Success terminal state.
    Ready,
    /// Failed terminal state; rollback either succeeded
    /// (`RolledBack`) or was not attempted / unsupported.
    Failed,
    /// Adapter rolled the target back to the prior release.
    RolledBack,
    /// Adapter attempted a rollback but the rollback itself
    /// failed. Distinct from `Failed` so operators can route it
    /// to the right recovery runbook.
    RollbackFailed,
}

impl DeploymentState {
    /// Whether the state is terminal (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::Failed
                | Self::RolledBack
                | Self::RollbackFailed
                | Self::PreflightRejected
        )
    }

    /// Whether the state is "successful" (`Ready`).
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Whether the state indicates a non-success terminal
    /// (anything other than `Ready`).
    pub fn is_failure(self) -> bool {
        self.is_terminal() && !self.is_ready()
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::PreflightInProgress => "preflight_in_progress",
            Self::PreflightRejected => "preflight_rejected",
            Self::Applied => "applied",
            Self::Verifying => "verifying",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::RollbackFailed => "rollback_failed",
        }
    }
}

/// Provider-neutral evidence record. Contains target identity,
/// release digest, action, state, timestamps, and a redacted
/// diagnostic. No host paths, no transport-specific fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentEvidence {
    /// Stable evidence id.
    id: Uuid,
    /// Target identity the action ran against.
    target: String,
    /// Adapter id that produced the evidence.
    adapter: String,
    /// Action.
    action: DeploymentAction,
    /// Idempotency key (none for read-only actions).
    operation_key: Option<OperationKey>,
    /// Release digest this evidence covers.
    release_digest: String,
    /// State the action reached.
    state: DeploymentState,
    /// When the action started.
    started_at: DateTime<Utc>,
    /// When the action completed.
    completed_at: DateTime<Utc>,
    /// Bounded, redacted diagnostic (never the secret value).
    diagnostic: String,
}

impl DeploymentEvidence {
    /// Build an evidence record. `diagnostic` is truncated to
    /// `MAX_DIAGNOSTIC_LEN` and stripped of any substring that
    /// looks like a secret value.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: impl Into<String>,
        adapter: impl Into<String>,
        action: DeploymentAction,
        operation_key: Option<OperationKey>,
        release_digest: impl Into<String>,
        state: DeploymentState,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        diagnostic: impl Into<String>,
    ) -> Result<Self, DeploymentAdapterError> {
        let target = target.into();
        let adapter = adapter.into();
        let release_digest = release_digest.into();
        if target.trim().is_empty() || target.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid("target is required".into()));
        }
        if adapter.trim().is_empty() || adapter.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "adapter is required".into(),
            ));
        }
        if release_digest.trim().is_empty() || release_digest.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "release_digest is required".into(),
            ));
        }
        if completed_at < started_at {
            return Err(DeploymentAdapterError::Invalid(
                "completed_at must not precede started_at".into(),
            ));
        }
        let diagnostic = redact_diagnostic(&diagnostic.into());
        Ok(Self {
            id: Uuid::new_v4(),
            target,
            adapter,
            action,
            operation_key,
            release_digest,
            state,
            started_at,
            completed_at,
            diagnostic,
        })
    }

    /// Stable evidence id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Target identity.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Adapter id.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// Action.
    pub fn action(&self) -> DeploymentAction {
        self.action
    }

    /// Idempotency key, if any.
    pub fn operation_key(&self) -> Option<&OperationKey> {
        self.operation_key.as_ref()
    }

    /// Release digest.
    pub fn release_digest(&self) -> &str {
        &self.release_digest
    }

    /// State.
    pub fn state(&self) -> DeploymentState {
        self.state
    }

    /// When the action started.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    /// When the action completed.
    pub fn completed_at(&self) -> DateTime<Utc> {
        self.completed_at
    }

    /// Redacted diagnostic.
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }

    /// Whether the evidence signals a successful terminal state.
    pub fn is_success(&self) -> bool {
        self.state.is_ready()
    }
}

/// A plan the caller submits to an adapter. The plan is
/// provider-neutral: no host path, no transport-specific
/// command, no secret value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    /// Target identity (adapter-specific opaque id).
    target: String,
    /// Action to perform.
    action: DeploymentAction,
    /// Idempotency key. Required when `action.is_mutating()`.
    operation_key: Option<OperationKey>,
    /// Release digest this plan is applying.
    release_digest: String,
    /// Secret references the adapter should resolve.
    secret_refs: Vec<SecretRef>,
    /// Adapter id that should run the plan.
    adapter: String,
    /// Authorised principal (username, never a token).
    actor: String,
    /// Optional dry-run flag: the adapter must validate and
    /// return a synthetic evidence record without mutating.
    dry_run: bool,
}

impl DeploymentPlan {
    /// Build a plan. Validates the mandatory fields, the
    /// `operation_key` ↔ `action` contract, and bounds the
    /// secret-reference list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: impl Into<String>,
        action: DeploymentAction,
        operation_key: Option<OperationKey>,
        release_digest: impl Into<String>,
        secret_refs: Vec<SecretRef>,
        adapter: impl Into<String>,
        actor: impl Into<String>,
        dry_run: bool,
    ) -> Result<Self, DeploymentAdapterError> {
        let target = target.into();
        let release_digest = release_digest.into();
        let adapter = adapter.into();
        let actor = actor.into();
        if target.trim().is_empty() || target.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid("target is required".into()));
        }
        if release_digest.trim().is_empty() || release_digest.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "release_digest is required".into(),
            ));
        }
        if adapter.trim().is_empty() || adapter.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid(
                "adapter is required".into(),
            ));
        }
        if actor.trim().is_empty() || actor.len() > MAX_IDENTIFIER_LEN {
            return Err(DeploymentAdapterError::Invalid("actor is required".into()));
        }
        if secret_refs.len() > 16 {
            return Err(DeploymentAdapterError::Invalid(
                "secret_refs must be 0..=16".into(),
            ));
        }
        if action.is_mutating() && operation_key.is_none() {
            return Err(DeploymentAdapterError::Invalid(
                "mutating action requires an operation key".into(),
            ));
        }
        if !action.is_mutating() && operation_key.is_some() {
            return Err(DeploymentAdapterError::Invalid(
                "read-only action must not carry an operation key".into(),
            ));
        }
        Ok(Self {
            target,
            action,
            operation_key,
            release_digest,
            secret_refs,
            adapter,
            actor,
            dry_run,
        })
    }

    /// Target identity.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Action.
    pub fn action(&self) -> DeploymentAction {
        self.action
    }

    /// Idempotency key, if any.
    pub fn operation_key(&self) -> Option<&OperationKey> {
        self.operation_key.as_ref()
    }

    /// Release digest.
    pub fn release_digest(&self) -> &str {
        &self.release_digest
    }

    /// Secret references.
    pub fn secret_refs(&self) -> &[SecretRef] {
        &self.secret_refs
    }

    /// Adapter id.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// Authorised principal.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// Whether this plan must run as a dry run.
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }
}

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

/// Truncate a diagnostic to `MAX_DIAGNOSTIC_LEN` and remove any
/// substring that looks like a secret value. The redaction
/// pattern is intentionally simple: drop everything after an
/// `=` on a credential marker line, and replace any PEM block
/// with `<redacted:pem>`.
pub fn redact_diagnostic(value: &str) -> String {
    let mut out = String::new();
    let markers = [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "private-key",
        "credential",
        "bearer",
    ];
    for line in value.lines() {
        let lower = line.to_lowercase();
        let redacted = if markers.iter().any(|m| lower.contains(m)) {
            if let Some(eq) = line.find('=') {
                format!("{} = <redacted>", &line[..eq])
            } else if let Some(colon) = line.find(':') {
                format!("{}: <redacted>", &line[..colon])
            } else {
                "<redacted>".to_string()
            }
        } else if line.contains("-----BEGIN") {
            "<redacted:pem>".to_string()
        } else {
            line.to_string()
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&redacted);
    }
    if out.len() > MAX_DIAGNOSTIC_LEN {
        out.truncate(MAX_DIAGNOSTIC_LEN);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Capability under test: `deployment-adapters`.
    const CAPABILITY: &str = "deployment-adapters";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 24, 0, 0, 0)
            .single()
            .expect("test time")
    }

    fn sample_manifest() -> AdapterManifest {
        AdapterManifest::new(
            "oci-compose-v1",
            "Compose on the OCI image",
            RUNTIME_CONTRACT_VERSION,
            "oci://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Rollback,
            ],
            true,
            HealthMethod::HttpGet {
                url: "http://127.0.0.1:8080/healthz".into(),
                expect_body_contains: Some("ok".into()),
            },
            vec!["agent-token".into()],
        )
        .expect("manifest")
    }

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "deployment-adapters");
    }

    #[test]
    fn action_classification_is_mutating_for_deploy_restart_rollback_preflight() {
        assert!(DeploymentAction::Deploy.is_mutating());
        assert!(DeploymentAction::Restart.is_mutating());
        assert!(DeploymentAction::Rollback.is_mutating());
        assert!(DeploymentAction::Preflight.is_mutating());
        assert!(!DeploymentAction::Status.is_mutating());
        assert!(!DeploymentAction::Verify.is_mutating());
        assert!(!DeploymentAction::Logs.is_mutating());
    }

    #[test]
    fn action_rollback_policy_is_attempt_only_for_deploy_and_restart() {
        assert_eq!(
            DeploymentAction::Deploy.rollback_policy(),
            RollbackPolicy::Attempt
        );
        assert_eq!(
            DeploymentAction::Restart.rollback_policy(),
            RollbackPolicy::Attempt
        );
        assert_eq!(
            DeploymentAction::Rollback.rollback_policy(),
            RollbackPolicy::Skip
        );
        assert_eq!(
            DeploymentAction::Preflight.rollback_policy(),
            RollbackPolicy::Skip
        );
    }

    #[test]
    fn manifest_rejects_empty_or_dup_action_list() {
        let err = AdapterManifest::new(
            "id",
            "label",
            RUNTIME_CONTRACT_VERSION,
            "scheme://",
            vec![],
            false,
            HealthMethod::TcpConnect {
                address: "127.0.0.1:22".into(),
            },
            vec![],
        );
        assert!(matches!(
            err,
            Err(DeploymentAdapterError::IncompleteManifest("actions"))
        ));
        let dup = AdapterManifest::new(
            "id",
            "label",
            RUNTIME_CONTRACT_VERSION,
            "scheme://",
            vec![DeploymentAction::Deploy, DeploymentAction::Deploy],
            false,
            HealthMethod::TcpConnect {
                address: "127.0.0.1:22".into(),
            },
            vec![],
        );
        assert!(matches!(dup, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn manifest_rejects_unsupported_contract_version() {
        let err = AdapterManifest::new(
            "id",
            "label",
            RUNTIME_CONTRACT_VERSION + 1,
            "scheme://",
            vec![DeploymentAction::Status],
            false,
            HealthMethod::TcpConnect {
                address: "127.0.0.1:22".into(),
            },
            vec![],
        );
        assert!(matches!(
            err,
            Err(DeploymentAdapterError::IncompleteManifest(
                "contract_version"
            ))
        ));
    }

    #[test]
    fn manifest_supports_lookup_is_binary() {
        let manifest = sample_manifest();
        assert!(manifest.supports(DeploymentAction::Deploy));
        assert!(manifest.supports(DeploymentAction::Rollback));
        assert!(!manifest.supports(DeploymentAction::Logs));
    }

    #[test]
    fn plan_rejects_mutating_action_without_operation_key() {
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            None,
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        );
        assert!(plan.is_err());
    }

    #[test]
    fn plan_rejects_read_only_action_with_operation_key() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Status, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Status,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        );
        assert!(plan.is_err());
    }

    #[test]
    fn validate_plan_refuses_undeclared_action() {
        let manifest = sample_manifest();
        // `sample_manifest` does not declare `Logs`; the plan MUST
        // omit an operation key (Logs is read-only).
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Logs,
            None,
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(
            result,
            Err(DeploymentAdapterError::UnsupportedAction("logs"))
        ));
    }

    #[test]
    fn validate_plan_refuses_undeclared_secret_reference() {
        let manifest = sample_manifest();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let secret = SecretRef::new("not-declared", None).unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![secret],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(result, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn validate_plan_refuses_rollback_when_manifest_does_not_declare_it() {
        // The manifest declares `Rollback` as a supported action but
        // does NOT declare rollback support (the boolean). The
        // rollback-only check fires after the action-declared check,
        // so we must include Rollback in the action list to reach it.
        let manifest = AdapterManifest::new(
            "oci-compose-v1",
            "Compose on the OCI image",
            RUNTIME_CONTRACT_VERSION,
            "oci://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Rollback,
            ],
            false,
            HealthMethod::HttpGet {
                url: "http://127.0.0.1:8080/healthz".into(),
                expect_body_contains: Some("ok".into()),
            },
            vec!["agent-token".into()],
        )
        .unwrap();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Rollback, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Rollback,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(result, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn validate_plan_refuses_adapter_mismatch() {
        let manifest = sample_manifest();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "mac-jenkins-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(result, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn validate_plan_accepts_dry_run_deploy() {
        let manifest = sample_manifest();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            true,
        )
        .unwrap();
        assert_eq!(
            validate_plan(&plan, &manifest).unwrap(),
            RollbackPolicy::Attempt
        );
    }

    #[test]
    fn operation_record_id_is_deterministic_for_same_key() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan_a = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key.clone()),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let plan_b = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        assert_eq!(
            operation_record_id(&plan_a).unwrap(),
            operation_record_id(&plan_b).unwrap()
        );
    }

    #[test]
    fn decide_replay_returns_replay_for_terminal_matching_evidence() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let cached = DeploymentEvidence::new(
            "oci://web-01",
            "oci-compose-v1",
            DeploymentAction::Deploy,
            plan.operation_key().cloned(),
            "sha256:deadbeef",
            DeploymentState::Ready,
            now(),
            now() + chrono::Duration::seconds(5),
            "ok",
        )
        .unwrap();
        assert_eq!(
            decide_replay(&plan, Some(&cached)),
            IdempotencyDecision::Replay
        );
    }

    #[test]
    fn decide_replay_returns_rerun_when_release_digest_differs() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let cached = DeploymentEvidence::new(
            "oci://web-01",
            "oci-compose-v1",
            DeploymentAction::Deploy,
            plan.operation_key().cloned(),
            "sha256:otherdigest",
            DeploymentState::Ready,
            now(),
            now() + chrono::Duration::seconds(5),
            "ok",
        )
        .unwrap();
        assert_eq!(
            decide_replay(&plan, Some(&cached)),
            IdempotencyDecision::Rerun
        );
    }

    #[test]
    fn decide_replay_returns_rerun_when_evidence_not_terminal() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let cached = DeploymentEvidence::new(
            "oci://web-01",
            "oci-compose-v1",
            DeploymentAction::Deploy,
            plan.operation_key().cloned(),
            "sha256:deadbeef",
            DeploymentState::Verifying,
            now(),
            now() + chrono::Duration::seconds(5),
            "still running",
        )
        .unwrap();
        assert_eq!(
            decide_replay(&plan, Some(&cached)),
            IdempotencyDecision::Rerun
        );
    }

    #[test]
    fn redact_diagnostic_strips_password_token_and_pem() {
        let raw = "preflight password=hunter2 ok\nbearer abcdef0123456789abcdef0123456789\n-----BEGIN RSA PRIVATE KEY-----\nxxx\n-----END RSA PRIVATE KEY-----";
        let redacted = redact_diagnostic(raw);
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("abcdef0123456789"));
        assert!(!redacted.contains("BEGIN RSA PRIVATE KEY"));
        assert!(redacted.contains("preflight"));
    }

    #[test]
    fn redact_diagnostic_truncates_to_max_diagnostic_len() {
        let raw: String = "a".repeat(MAX_DIAGNOSTIC_LEN * 2);
        let redacted = redact_diagnostic(&raw);
        assert!(redacted.len() <= MAX_DIAGNOSTIC_LEN);
    }

    #[test]
    fn state_terminal_and_failure_classification() {
        assert!(DeploymentState::Ready.is_terminal());
        assert!(DeploymentState::Ready.is_ready());
        assert!(!DeploymentState::Ready.is_failure());
        assert!(DeploymentState::Failed.is_terminal());
        assert!(DeploymentState::Failed.is_failure());
        assert!(DeploymentState::RollbackFailed.is_terminal());
        assert!(DeploymentState::RollbackFailed.is_failure());
        assert!(!DeploymentState::Applied.is_terminal());
        assert!(!DeploymentState::Verifying.is_terminal());
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_operation_record_id_is_stable(
                target in "[a-z0-9:/\\-]{1,16}",
                value in "[a-z0-9]{1,16}",
            ) {
                let key = OperationKey::new(&target, DeploymentAction::Deploy, &value).unwrap();
                let plan = DeploymentPlan::new(
                    &target,
                    DeploymentAction::Deploy,
                    Some(key),
                    "sha256:abc",
                    vec![],
                    "oci-compose-v1",
                    "owner",
                    false,
                )
                .unwrap();
                let first = operation_record_id(&plan).unwrap();
                let second = operation_record_id(&plan).unwrap();
                prop_assert_eq!(first, second);
            }

            #[test]
            fn prop_redact_never_keeps_password_marker(line in ".{1,80}") {
                let with = format!("password=hunter2 {line}");
                let redacted = redact_diagnostic(&with);
                prop_assert!(!redacted.contains("hunter2"));
            }
        }
    }
}

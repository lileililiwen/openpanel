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
//!
//! Module shape:
//! - `types` — the data shapes (manifest, plan, evidence,
//!   action, state, idempotency, health, secret reference) and
//!   the pure `redact_diagnostic` helper.
//! - `logic` — the validate / decide / record-id helpers
//!   and the [`DeploymentAdapter`] trait.
//! - `tests` — unit + property tests for both.
//!
//! The `pub use` re-exports below preserve the historical single-
//! file import paths so existing callers (`openpanel_domain::*`)
//! do not need to change.

mod logic;
mod tests;
mod types;

pub use logic::{
    DeploymentAdapter, IdempotencyDecision, decide_replay, operation_record_id, validate_plan,
};
pub use types::{
    AdapterManifest, DeploymentAction, DeploymentAdapterError, DeploymentEvidence, DeploymentPlan,
    DeploymentState, HealthMethod, MAX_DECLARED_ACTIONS, MAX_DIAGNOSTIC_LEN, MAX_IDENTIFIER_LEN,
    OperationKey, RUNTIME_CONTRACT_VERSION, RollbackPolicy, SecretRef, redact_diagnostic,
};

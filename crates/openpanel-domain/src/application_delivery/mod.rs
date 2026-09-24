//! Application delivery bounded context: provider-neutral
//! lifecycle for source- or image-based applications above the
//! existing site Git, container, preview, and release services.
//!
//! The application delivery aggregate owns:
//! - the application record (name, source kind, source ref, build
//!   hint, environments),
//! - the environment record (production vs preview, secret-set
//!   reference, quota ceiling, expiry),
//! - the immutable release record (digest, source pointer, build
//!   metadata, audit trail),
//! - the lifecycle state machine (`Pending -> Building ->
//!   Deploying -> Healthy | Failed | RolledBack`),
//! - the promotion + rollback operators,
//! - the bounded `redact_diagnostic` helper for the lifecycle log.
//!
//! The application layer wires these into the existing
//! `git_deployment`, `container_runtime`, `site_staging`, and
//! `deployment_adapters` bounded contexts. This module does not
//! re-implement transport, host policy, or secret storage — it
//! owns the *typed lifecycle* and the contract the rest of the
//! system speaks.
//!
//! I/O-free: no sqlx, no axum, no tokio, no subprocess.
//!
//! Module shape:
//! - `types`     — the data shapes (Application, ApplicationSource,
//!   Environment, Release, ReleaseState, PromotionRequest,
//!   RollbackRequest, SecretSetRef, ApplicationDeliveryError)
//!   and the pure redaction helper.
//! - `logic`     — the validate / decide / record-id helpers
//!   and the typed `BuildAdapter` / `RuntimeAdapter` ports.
//! - `tests`     — unit + property tests for both.
//!
//! The `pub use` re-exports below preserve the historical single-
//! file import paths so existing callers (`openpanel_domain::*`)
//! do not need to change.

mod logic;
mod tests;
mod types;

pub use logic::{
    BuildAdapter, MAX_APPLICATION_NAME, MAX_DIGEST_LEN, MAX_REF_LEN, ReleaseLifecycle,
    ReleaseOutcome, RuntimeAdapter, build_release_identity, can_promote, can_rollback,
    is_mutable_digest, resolve_environment, validate_environment_isolation,
};
pub use types::{
    Application, ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind, AuditTag,
    Environment, EnvironmentKind, PromotionRequest, Release, ReleaseDigest, ReleaseState,
    RollbackRequest, SecretSetRef, redact_diagnostic, validate_source_ref,
};

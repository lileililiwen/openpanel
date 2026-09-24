//! Verified-service-catalog bounded context: signed, declarative,
//! reviewable ecosystem service templates with safe lifecycle
//! operations.
//!
//! The catalog's unit of contribution is a [`ServiceManifest`]: a
//! versioned, publisher-signed declaration of an OCI/Compose service
//! with compatibility, permissions, resources, storage, health,
//! backup, and upgrade metadata. Signature verification
//! ([`verify_manifest_signature`]) MUST succeed before any planning
//! ([`plan_service_install`]); a failed verification maps the entry
//! to [`TrustState::Blocked`] and [`ensure_installable`] refuses it.
//!
//! Planning is a pure preview: it reveals the effective ports,
//! storage, network access, resource limits, credential references,
//! and data effects, and fails with a specific [`PlanConflict`] —
//! performing no mutation — when the target host policy would be
//! exceeded.
//!
//! Lifecycle operations (install, upgrade, backup, health, rollback,
//! remove) are represented as observable [`ServiceJob`] records with
//! bounded retries, explicit [`JobOutcome`] states, and operator
//! guidance on failure. Provisioning is delegated behind the
//! [`CatalogProvisioner`] port so no catalog template can execute
//! opaque host scripts; the app layer wires the port to the existing
//! software-center and container-runtime services.
//!
//! Trust revocation ([`RevocationList`]) refuses revoked publishers
//! or artifacts even when the entry remains cached locally.
//!
//! I/O-free: no sqlx, no axum, no tokio, no subprocess.
//!
//! Module shape:
//! - `manifest` — the signed contribution shapes (publisher, digest,
//!   compatibility, resources, permissions, health, backup, upgrade,
//!   manifest, install plan, revocation list).
//! - `types`    — the policy / lifecycle / trust shapes (host policy,
//!   plan conflict, job phase/outcome/job, trust state, error).
//! - `logic`    — the signature / validation / planning / lifecycle /
//!   installability helpers and the [`CatalogProvisioner`] port.
//! - `tests`    — unit + property tests for all of the above.

mod logic;
mod manifest;
mod tests;
mod types;

pub use logic::{
    CatalogProvisioner, ProvisionOutcome, can_retry, check_target_compatibility,
    ensure_installable, manifest_signature, plan_service_install, service_job_transition_allowed,
    verify_manifest_signature,
};
pub use manifest::{
    PublisherIdentity, ResourceRequest, ServiceBackupPolicy, ServiceCompatibility,
    ServiceHealthProbe, ServiceManifest, ServicePermissions, ServiceUpgradePolicy, SourceDigest,
};
pub use types::{
    HostPolicy, JobOutcome, JobPhase, PlanConflict, RevocationList, ServiceJob, ServicePlan,
    TrustState, VerifiedServiceCatalogError,
};

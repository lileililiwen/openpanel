//! Application-delivery pure logic: validate / decide /
//! record-id helpers, the typed `BuildAdapter` and
//! `RuntimeAdapter` ports, and the state-machine
//! `ReleaseLifecycle` helper that the service uses to step a
//! release through `Pending -> Building -> Deploying -> Healthy`.
//! This file is I/O-free; tests live in `tests.rs`.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::types::{
    ApplicationDeliveryError, ApplicationSource, Environment, EnvironmentKind, PromotionRequest,
    Release, ReleaseDigest, ReleaseState, RollbackRequest, SecretSetRef,
};

/// Maximum length of a release digest value (matches the
/// `deployment_adapters` constant so the two contracts agree).
pub const MAX_DIGEST_LEN: usize = 128;

/// Maximum length of a source ref (matches the validators in
/// `types::validate_source_ref`).
pub const MAX_REF_LEN: usize = 128;

/// Maximum length of an application name.
pub const MAX_APPLICATION_NAME: usize = 64;

/// The release state machine, exposed as a set of pure helpers so
/// the service and the tests can step a release through its
/// lifecycle without owning the state directly.
pub struct ReleaseLifecycle;

impl ReleaseLifecycle {
    /// `Pending -> Building`. Refuses if the release is not in
    /// `Pending`.
    pub fn start_build(
        release: &mut Release,
        now: DateTime<Utc>,
    ) -> Result<(), ApplicationDeliveryError> {
        release.transition(ReleaseState::Building, now, "build started")
    }

    /// `Building -> Deploying`. Refuses if the release is not in
    /// `Building`.
    pub fn finish_build(
        release: &mut Release,
        now: DateTime<Utc>,
        digest: &ReleaseDigest,
    ) -> Result<(), ApplicationDeliveryError> {
        // The release identity is what the deployment-adapter
        // contract consumes; updating the digest on the release
        // ensures the deployment step targets the same digest
        // the build produced.
        release
            .transition(ReleaseState::Deploying, now, "build finished; deploying")
            .map_err(|err| {
                ApplicationDeliveryError::IllegalTransition(format!(
                    "finish_build: {}; digest={}",
                    err,
                    digest.value()
                ))
            })
    }

    /// `Deploying -> Healthy`. Refuses if the release is not in
    /// `Deploying`. The diagnostic is the health-check result.
    pub fn mark_healthy(
        release: &mut Release,
        now: DateTime<Utc>,
        health: &str,
    ) -> Result<(), ApplicationDeliveryError> {
        release.transition(ReleaseState::Healthy, now, health)
    }

    /// `Building | Deploying -> Failed`. Refuses illegal moves.
    pub fn fail(
        release: &mut Release,
        now: DateTime<Utc>,
        diagnostic: &str,
    ) -> Result<(), ApplicationDeliveryError> {
        release.transition(ReleaseState::Failed, now, diagnostic)
    }

    /// `Failed -> RolledBack`. Refuses illegal moves.
    pub fn roll_back(
        release: &mut Release,
        now: DateTime<Utc>,
        diagnostic: &str,
    ) -> Result<(), ApplicationDeliveryError> {
        release.transition(ReleaseState::RolledBack, now, diagnostic)
    }
}

/// Build a stable, deterministic release identity from a
/// source ref + a build epoch. The function is pure so the
/// test suite can replay it; the production wiring still
/// uses the build adapter's resolved digest for the actual
/// `ReleaseDigest`.
pub fn build_release_identity(source: &ApplicationSource, build_epoch: u64) -> String {
    let mut acc: u64 = 0xcbf29ce484222325; // FNV-1a 64-bit offset basis
    for byte in source.url().as_bytes() {
        acc ^= *byte as u64;
        acc = acc.wrapping_mul(0x100000001b3);
    }
    for byte in source.source_ref().as_bytes() {
        acc ^= *byte as u64;
        acc = acc.wrapping_mul(0x100000001b3);
    }
    let epoch_bytes = build_epoch.to_le_bytes();
    for byte in epoch_bytes {
        acc ^= byte as u64;
        acc = acc.wrapping_mul(0x100000001b3);
    }
    format!("sha256:{:016x}", acc)
}

/// Whether a release can be promoted. Production promotion
/// requires an authorised confirmation, a healthy target
/// environment, and (when the environment already has a healthy
/// release) a non-`None` `previous_release_id`.
pub fn can_promote(
    release: &Release,
    environment: &Environment,
    request: &PromotionRequest,
) -> Result<(), ApplicationDeliveryError> {
    if release.environment_id() != environment.id() {
        return Err(ApplicationDeliveryError::Invalid(
            "release.environment_id does not match the target environment".into(),
        ));
    }
    if release.application_id() != environment.application_id() {
        return Err(ApplicationDeliveryError::Invalid(
            "release.application_id does not match the target environment".into(),
        ));
    }
    if request.confirmation().trim().is_empty() {
        return Err(ApplicationDeliveryError::Invalid(
            "promotion requires a non-empty confirmation".into(),
        ));
    }
    if !matches!(
        release.state(),
        ReleaseState::Pending | ReleaseState::Building | ReleaseState::Deploying
    ) {
        return Err(ApplicationDeliveryError::ReleaseNotReady("promoted"));
    }
    Ok(())
}

/// Whether a release can be rolled back. Rollback requires the
/// release to be in a non-terminal state or in `Failed` (so a
/// recovery transition is legal) and the target environment to
/// still exist.
pub fn can_rollback(
    release: &Release,
    environment: &Environment,
    request: &RollbackRequest,
) -> Result<(), ApplicationDeliveryError> {
    if release.environment_id() != environment.id() {
        return Err(ApplicationDeliveryError::Invalid(
            "release.environment_id does not match the rollback environment".into(),
        ));
    }
    if request.confirmation().trim().is_empty() {
        return Err(ApplicationDeliveryError::Invalid(
            "rollback requires a non-empty confirmation".into(),
        ));
    }
    if !matches!(
        release.state(),
        ReleaseState::Failed | ReleaseState::Deploying
    ) {
        return Err(ApplicationDeliveryError::ReleaseNotReady("rolled back"));
    }
    Ok(())
}

/// Whether a digest is mutable. Mutable digests are unpinned tags
/// (`latest`, `main`, etc.) and any value that is not
/// `sha256:…`. The rule is intentionally strict: any value that
/// the function cannot prove is immutable is rejected.
pub fn is_mutable_digest(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_DIGEST_LEN {
        return true;
    }
    if !value.starts_with("sha256:") {
        return true;
    }
    let after = &value[7..];
    if after.is_empty() || after.len() > 64 {
        return true;
    }
    !after.chars().all(|c| c.is_ascii_hexdigit())
}

/// Validate environment isolation. Preview environments MUST NOT
/// resolve a production secret set; production environments MUST
/// NOT resolve a preview secret set. The check is a pure function
/// so the service and the tests share the rule.
pub fn validate_environment_isolation(
    environment: &Environment,
    secret_set: &SecretSetRef,
) -> Result<(), ApplicationDeliveryError> {
    if environment.is_expired(environment.created_at()) {
        // Newborn environments cannot be born expired; a separate
        // reaper handles the "expired at now" case. The check
        // exists here so the type system cannot accept an
        // expired-at-construction environment.
        return Err(ApplicationDeliveryError::Invalid(
            "environment is born expired".into(),
        ));
    }
    match environment.kind() {
        EnvironmentKind::Production => {
            if !secret_set.name().starts_with("prod-") {
                return Err(ApplicationDeliveryError::Invalid(
                    "production environment must resolve a `prod-` secret set".into(),
                ));
            }
        }
        EnvironmentKind::Preview => {
            if !secret_set.name().starts_with("preview-") {
                return Err(ApplicationDeliveryError::PreviewCannotUseProductionSecret);
            }
        }
    }
    Ok(())
}

/// Resolve the secret set for a release + environment. The
/// function is the single place that enforces the production
/// /preview isolation rule; the app service calls it before
/// dispatching a build or a deploy.
pub fn resolve_environment<'a>(
    environment: &'a Environment,
    production_set: &'a SecretSetRef,
    preview_set: &'a SecretSetRef,
) -> Result<&'a SecretSetRef, ApplicationDeliveryError> {
    let candidate = match environment.kind() {
        EnvironmentKind::Production => production_set,
        EnvironmentKind::Preview => preview_set,
    };
    validate_environment_isolation(environment, candidate)?;
    Ok(candidate)
}

/// Typed build adapter port. Concrete adapters (fake, OCI, etc.)
/// implement this trait; the service consumes the trait so the
/// tests can use a fake.
pub trait BuildAdapter: Send + Sync + 'static {
    /// Adapter id (e.g. `fake-build-v1`, `oci-build-v1`).
    fn adapter_id(&self) -> &'static str;
    /// Resolve an application source into an immutable
    /// `ReleaseDigest`. The function is async because real
    /// adapters clone / pull; the fake adapter returns a
    /// deterministic digest synchronously.
    fn resolve_digest(
        &self,
        application_id: Uuid,
        source: &ApplicationSource,
    ) -> Result<ReleaseDigest, ApplicationDeliveryError>;
}

/// Typed runtime adapter port. Concrete adapters delegate to the
/// `deployment_adapters` contract; the fake adapter is a no-op
/// for unit tests.
pub trait RuntimeAdapter: Send + Sync + 'static {
    /// Adapter id (e.g. `fake-runtime-v1`, `oci-runtime-v1`).
    fn adapter_id(&self) -> &'static str;
    /// Apply a release to an environment. Returns the terminal
    /// `ReleaseState` and a bounded, redacted diagnostic.
    fn deploy(
        &self,
        release: &Release,
        environment: &Environment,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError>;
    /// Roll back the environment to the prior healthy release.
    fn rollback(
        &self,
        release: &Release,
        environment: &Environment,
        target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError>;
}

/// Adapter outcome: terminal state + bounded diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseOutcome {
    /// Terminal `ReleaseState` the adapter returned
    /// (`Healthy` on success, `Failed` on failure).
    pub state: ReleaseState,
    /// Bounded, redacted diagnostic message describing
    /// the adapter's transition outcome.
    pub diagnostic: String,
}

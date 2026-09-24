//! Verified-service-catalog pure logic: manifest signing and
//! signature verification, target compatibility, install planning
//! with resource-conflict detection, lifecycle job decisions, and
//! the [`CatalogProvisioner`] port the app layer wires to the
//! existing software-center / container-runtime services.
//!
//! The signature model binds the *publisher key* to the *pinned
//! source digest* — never to template text — so publisher signatures
//! and image digests are verified independently of manifest content
//! (a tampered digest fails verification even when every other
//! field is well-formed). The conformance scheme is an HMAC-SHA256
//! over the digest under the publisher fingerprint, reusing the
//! domain's `hmac` + `sha2` primitives (same pattern as
//! `api_tokens` and `notifications`); a production publisher registry
//! can swap in asymmetric verification without touching the trust
//! vocabulary.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::verified_service_catalog::manifest::{PublisherIdentity, ServiceManifest, SourceDigest};
use crate::verified_service_catalog::types::{
    HostPolicy, JobOutcome, PlanConflict, RevocationList, ServiceJob, ServicePlan, TrustState,
    VerifiedServiceCatalogError,
};

type HmacSha256 = Hmac<Sha256>;

/// Compute the expected signature of `digest` under `publisher`'s
/// fingerprint. Deterministic and bounded: the output is lowercase
/// hex, 64 characters.
pub fn manifest_signature(publisher: &PublisherIdentity, digest: &SourceDigest) -> String {
    // Same pattern as `api_tokens::TokenCredential::hash`: an
    // HMAC-SHA256 key accepts any byte string, so the only failure
    // mode is impossible here.
    #[allow(clippy::expect_used)]
    let mut mac = HmacSha256::new_from_slice(publisher.fingerprint().as_bytes())
        .expect("HMAC-SHA256 accepts every key byte string");
    mac.update(digest.as_str().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Verify a manifest's presented signature.
///
/// Returns [`TrustState::Verified`] when the signature verifies
/// against a curated trusted publisher's fingerprint, and
/// [`TrustState::Community`] when it verifies against the
/// manifest's own declared fingerprint but no curated publisher
/// matches (self-signed). Any mismatch returns
/// [`VerifiedServiceCatalogError::InvalidSignature`] — callers map
/// that to [`TrustState::Blocked`] and refuse planning and install.
pub fn verify_manifest_signature(
    manifest: &ServiceManifest,
    trusted: &[PublisherIdentity],
) -> Result<TrustState, VerifiedServiceCatalogError> {
    let presented = manifest.signature();
    // Verification against the manifest's own declared publisher.
    let expected = manifest_signature(manifest.publisher(), manifest.digest());
    if !signature_matches(presented, &expected) {
        return Err(VerifiedServiceCatalogError::InvalidSignature);
    }
    // A matching curated publisher (id and key) upgrades the entry
    // to the verified tier.
    let curated = trusted.iter().any(|publisher| {
        publisher.id() == manifest.publisher().id()
            && publisher.fingerprint() == manifest.publisher().fingerprint()
    });
    Ok(if curated {
        TrustState::Verified
    } else {
        TrustState::Community
    })
}

/// Case-insensitive comparison of a presented signature against the
/// expected hex. The comparison is length-gated first so a garbage
/// presentation never passes.
fn signature_matches(presented: &str, expected: &str) -> bool {
    let lower = presented.to_ascii_lowercase();
    let hex_ok = lower.len() == 64
        && lower
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    hex_ok && lower == expected
}

/// Whether the manifest declares compatibility with the policy's
/// target (os, arch) pair.
pub fn check_target_compatibility(
    manifest: &ServiceManifest,
    policy: &HostPolicy,
) -> Result<(), VerifiedServiceCatalogError> {
    if manifest
        .compatibility()
        .supports(policy.target_id(), policy.target_arch())
    {
        Ok(())
    } else {
        Err(VerifiedServiceCatalogError::IncompatibleTarget(format!(
            "{}:{} is not in the declared compatibility set",
            policy.target_id(),
            policy.target_arch()
        )))
    }
}

/// Build the reviewable install plan for `manifest` against
/// `policy`, or the first specific conflict that blocks planning.
///
/// The function is pure: a returned conflict means no mutation was
/// or will be performed — the caller never receives a half-built
/// plan. Checks run in a fixed order (ports, storage, CPU, memory)
/// so the same input always reports the same conflict.
pub fn plan_service_install(
    manifest: &ServiceManifest,
    policy: &HostPolicy,
) -> Result<ServicePlan, PlanConflict> {
    let permissions = manifest.permissions();
    let resources = manifest.resources();
    for port in permissions.ports() {
        if !policy.allowed_ports().contains(port) {
            return Err(PlanConflict::PortNotAllowed { port: *port });
        }
    }
    if resources.storage_bytes() > policy.free_storage_bytes() {
        return Err(PlanConflict::StorageQuotaExceeded {
            requested: resources.storage_bytes(),
            available: policy.free_storage_bytes(),
        });
    }
    if resources.cpu_millis() > policy.max_cpu_millis() {
        return Err(PlanConflict::CpuQuotaExceeded {
            requested: resources.cpu_millis(),
            limit: policy.max_cpu_millis(),
        });
    }
    if resources.memory_bytes() > policy.max_memory_bytes() {
        return Err(PlanConflict::MemoryQuotaExceeded {
            requested: resources.memory_bytes(),
            limit: policy.max_memory_bytes(),
        });
    }
    Ok(ServicePlan::from_manifest(manifest))
}

/// Final installability gate applied to every catalog request:
/// refuses blocked entries, revoked trust states, and entries whose
/// publisher or artifact appears in the active [`RevocationList`] —
/// even when the entry remains cached locally.
pub fn ensure_installable(
    trust: TrustState,
    manifest: &ServiceManifest,
    revocations: &RevocationList,
) -> Result<(), VerifiedServiceCatalogError> {
    match trust {
        TrustState::Blocked => Err(VerifiedServiceCatalogError::Blocked),
        TrustState::Revoked => Err(VerifiedServiceCatalogError::Revoked(format!(
            "entry {}",
            manifest.id()
        ))),
        TrustState::Verified | TrustState::Community => {
            if revocations.is_publisher_revoked(manifest.publisher().id()) {
                return Err(VerifiedServiceCatalogError::Revoked(format!(
                    "publisher {}",
                    manifest.publisher().id()
                )));
            }
            if revocations.is_digest_revoked(manifest.digest().as_str()) {
                return Err(VerifiedServiceCatalogError::Revoked(format!(
                    "digest {}",
                    manifest.digest().as_str()
                )));
            }
            Ok(())
        }
    }
}

/// Whether a job may run (another) attempt: bounded by the
/// declared retry ceiling and refused once the job is terminal.
pub fn can_retry(job: &ServiceJob) -> bool {
    matches!(job.outcome(), JobOutcome::Pending | JobOutcome::Failed)
        && job.attempts() < job.max_attempts()
}

/// The lifecycle job state machine:
/// `Pending -> Running -> (Succeeded | Failed)`;
/// `Failed -> Running` (retry) or `Failed -> RolledBack`.
/// `Succeeded` and `RolledBack` are terminal.
pub fn service_job_transition_allowed(from: JobOutcome, to: JobOutcome) -> bool {
    matches!(
        (from, to),
        (JobOutcome::Pending, JobOutcome::Running)
            | (JobOutcome::Running, JobOutcome::Succeeded)
            | (JobOutcome::Running, JobOutcome::Failed)
            | (JobOutcome::Failed, JobOutcome::Running)
            | (JobOutcome::Failed, JobOutcome::RolledBack)
    )
}

/// Outcome of a provisioner health probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionOutcome {
    /// Whether the declared probe passed.
    pub healthy: bool,
    /// Transport diagnostic (the app layer redacts before display).
    pub diagnostic: String,
}

/// The provisioning port for catalog lifecycle operations. The app
/// layer wires this to the existing software-center install wizard
/// and container-runtime services; templates never execute opaque
/// host scripts. All methods are synchronous contracts — the app
/// service runs them inside observable jobs with bounded retries.
pub trait CatalogProvisioner: Send + Sync + 'static {
    /// Stable provisioner identity for audit attribution.
    fn provider_id(&self) -> &'static str;
    /// Provision the pinned digest behind the reviewed plan.
    fn install(&self, plan: &ServicePlan) -> Result<(), VerifiedServiceCatalogError>;
    /// Capture a backup of the installed service.
    fn backup(&self, entry_id: &str) -> Result<(), VerifiedServiceCatalogError>;
    /// Run the declared health probe for the entry.
    fn health_check(&self, entry_id: &str)
    -> Result<ProvisionOutcome, VerifiedServiceCatalogError>;
    /// Remove the service and its declared data effects.
    fn remove(&self, entry_id: &str) -> Result<(), VerifiedServiceCatalogError>;
}

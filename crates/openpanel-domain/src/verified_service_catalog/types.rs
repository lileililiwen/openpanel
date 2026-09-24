//! Verified-service-catalog lifecycle and policy shapes: the typed
//! error, the trust state, the host policy, planning conflicts, the
//! reviewable install plan, the revocation list, and the observable
//! lifecycle job. The manifest-side data shapes (publisher, digest,
//! compatibility, resources, permissions, health, backup, upgrade,
//! and the [`ServiceManifest`] aggregate) live in `manifest.rs`.
//!
//! I/O-free. Every constructor validates its inputs so a hostile
//! or malformed manifest cannot be built into the catalog in a
//! planning-ready state.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::deployment_adapters::redact_diagnostic;
use crate::verified_service_catalog::manifest::ServiceManifest;

/// Errors raised by the verified-service-catalog domain.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerifiedServiceCatalogError {
    /// The manifest failed schema validation.
    #[error("invalid manifest: {0}")]
    Invalid(String),
    /// The presented signature does not match the signature computed
    /// over the manifest's pinned source digest under its publisher
    /// key. The caller maps this to `TrustState::Blocked`.
    #[error("manifest signature does not verify")]
    InvalidSignature,
    /// The publisher or artifact is revoked; a cached copy does not
    /// make it installable.
    #[error("revoked: {0}")]
    Revoked(String),
    /// The entry is blocked (failed signature verification) and
    /// refuses planning and install.
    #[error("entry is blocked")]
    Blocked,
    /// The manifest does not declare compatibility with the target.
    #[error("incompatible target: {0}")]
    IncompatibleTarget(String),
    /// A provisioner port reported a transport/host failure.
    #[error("provisioning failed: {0}")]
    Provision(String),
}

/// Trust classification of a catalog entry. Mirrors the
/// `software-center-trust` fail-closed vocabulary: only `Verified`
/// and `Community` entries may be planned or installed; `Revoked`
/// and `Blocked` always refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    /// Signature verified against a curated (trusted) publisher.
    Verified,
    /// Signature verified against the manifest's own declared key,
    /// but the publisher is not in the curated set.
    Community,
    /// Publisher or artifact revoked after publication.
    Revoked,
    /// Signature verification failed; the entry cannot be planned
    /// or installed.
    Blocked,
}

impl TrustState {
    /// Whether an entry in this state may be planned or installed.
    pub fn is_installable(self) -> bool {
        matches!(self, TrustState::Verified | TrustState::Community)
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            TrustState::Verified => "verified",
            TrustState::Community => "community",
            TrustState::Revoked => "revoked",
            TrustState::Blocked => "blocked",
        }
    }
}

/// The target host policy an install plan must fit within: the
/// target OS/arch pair (mirroring the portable-runtime target
/// manifest), the operator-allowed port set, and the resource
/// ceilings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPolicy {
    target_id: String,
    target_arch: String,
    allowed_ports: BTreeSet<u16>,
    max_cpu_millis: u32,
    max_memory_bytes: u64,
    free_storage_bytes: u64,
}

impl HostPolicy {
    /// Validate and build a host policy. Ports must be in
    /// `1..=65535`; ids must be non-empty.
    pub fn new(
        target_id: impl Into<String>,
        target_arch: impl Into<String>,
        allowed_ports: Vec<u16>,
        max_cpu_millis: u32,
        max_memory_bytes: u64,
        free_storage_bytes: u64,
    ) -> Result<Self, VerifiedServiceCatalogError> {
        let target_id = target_id.into();
        let target_arch = target_arch.into();
        if target_id.trim().is_empty() || target_arch.trim().is_empty() {
            return Err(VerifiedServiceCatalogError::Invalid(
                "host policy requires a target id and arch".into(),
            ));
        }
        let mut set = BTreeSet::new();
        for port in allowed_ports {
            if port == 0 {
                return Err(VerifiedServiceCatalogError::Invalid(
                    "port 0 is not allowed".into(),
                ));
            }
            set.insert(port);
        }
        Ok(Self {
            target_id,
            target_arch,
            allowed_ports: set,
            max_cpu_millis,
            max_memory_bytes,
            free_storage_bytes,
        })
    }

    /// Target OS identifier (e.g. `debian`).
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    /// Target CPU architecture (e.g. `amd64`).
    pub fn target_arch(&self) -> &str {
        &self.target_arch
    }

    /// Operator-allowed listener ports.
    pub fn allowed_ports(&self) -> &BTreeSet<u16> {
        &self.allowed_ports
    }

    /// CPU ceiling in millicores.
    pub fn max_cpu_millis(&self) -> u32 {
        self.max_cpu_millis
    }

    /// Memory ceiling in bytes.
    pub fn max_memory_bytes(&self) -> u64 {
        self.max_memory_bytes
    }

    /// Free storage available on the target.
    pub fn free_storage_bytes(&self) -> u64 {
        self.free_storage_bytes
    }
}

/// A specific planning refusal. The conflict names the axis and the
/// numbers so the operator sees exactly what to change; planning
/// performs no mutation when a conflict is returned.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanConflict {
    /// The service requests a port outside the host policy.
    #[error("port {port} is outside the host policy")]
    PortNotAllowed {
        /// The offending port.
        port: u16,
    },
    /// The service storage request exceeds the free target storage.
    #[error("storage request {requested} bytes exceeds {available} bytes free")]
    StorageQuotaExceeded {
        /// Requested storage bytes.
        requested: u64,
        /// Available storage bytes.
        available: u64,
    },
    /// The service CPU request exceeds the policy ceiling.
    #[error("cpu request {requested} millicores exceeds limit {limit}")]
    CpuQuotaExceeded {
        /// Requested CPU (millicores).
        requested: u32,
        /// Policy ceiling (millicores).
        limit: u32,
    },
    /// The service memory request exceeds the policy ceiling.
    #[error("memory request {requested} bytes exceeds limit {limit}")]
    MemoryQuotaExceeded {
        /// Requested memory bytes.
        requested: u64,
        /// Policy ceiling bytes.
        limit: u64,
    },
}

/// The reviewable install preview: the effective ports, storage,
/// network access, resource limits, credential references, and data
/// effects a service will have once confirmed. Built only by
/// [`plan_service_install`](super::plan_service_install), so an
/// un-planned (conflicting or unverified) service cannot yield one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicePlan {
    entry_id: String,
    digest: String,
    ports: Vec<u16>,
    public_network: bool,
    credential_refs: Vec<String>,
    data_effects: Vec<String>,
    storage_bytes: u64,
    cpu_millis: u32,
    memory_bytes: u64,
}

impl ServicePlan {
    /// Build the plan from a validated manifest. The plan is the
    /// manifest's declared effect projected onto the host: entry
    /// identity, pinned digest, ports, network access, credential
    /// references, data effects, and the resource ceilings.
    pub(crate) fn from_manifest(manifest: &ServiceManifest) -> Self {
        let permissions = manifest.permissions();
        let resources = manifest.resources();
        Self {
            entry_id: manifest.id().to_owned(),
            digest: manifest.digest().as_str().to_owned(),
            ports: permissions.ports().to_vec(),
            public_network: permissions.public_network(),
            credential_refs: permissions.credential_refs().to_vec(),
            data_effects: permissions.data_effects().to_vec(),
            storage_bytes: resources.storage_bytes(),
            cpu_millis: resources.cpu_millis(),
            memory_bytes: resources.memory_bytes(),
        }
    }

    /// Planned entry id.
    pub fn entry_id(&self) -> &str {
        &self.entry_id
    }

    /// Planned immutable digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Effective ports.
    pub fn ports(&self) -> Vec<u16> {
        self.ports.clone()
    }

    /// Whether the install will expose a public listener.
    pub fn public_network(&self) -> bool {
        self.public_network
    }

    /// Credential references (ids only, never values).
    pub fn credential_refs(&self) -> Vec<String> {
        self.credential_refs.clone()
    }

    /// Data effects the install will perform.
    pub fn data_effects(&self) -> Vec<String> {
        self.data_effects.clone()
    }

    /// Storage the install will consume.
    pub fn storage_bytes(&self) -> u64 {
        self.storage_bytes
    }

    /// CPU ceiling the install will enforce.
    pub fn cpu_millis(&self) -> u32 {
        self.cpu_millis
    }

    /// Memory ceiling the install will enforce.
    pub fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }
}

/// The set of revoked publishers and revoked artifact digests. The
/// catalog checks this list at request time, so a revoked entry
/// cannot be installed from cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RevocationList {
    publishers: BTreeSet<String>,
    digests: BTreeSet<String>,
}

impl RevocationList {
    /// An empty revocation list.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Revoke a publisher by id (and, transitively, every entry it
    /// signed).
    pub fn revoke_publisher_id(&mut self, publisher_id: impl Into<String>) {
        self.publishers.insert(publisher_id.into());
    }

    /// Revoke a single artifact by its source digest.
    pub fn revoke_artifact_digest(&mut self, digest: impl Into<String>) {
        self.digests.insert(digest.into());
    }

    /// Whether the publisher id is revoked.
    pub fn is_publisher_revoked(&self, publisher_id: &str) -> bool {
        self.publishers.contains(publisher_id)
    }

    /// Whether the artifact digest is revoked.
    pub fn is_digest_revoked(&self, digest: &str) -> bool {
        self.digests.contains(digest)
    }

    /// Whether the manifest is blocked by any revocation.
    pub fn blocks_manifest(&self, manifest: &ServiceManifest) -> bool {
        self.is_publisher_revoked(manifest.publisher().id())
            || self.is_digest_revoked(manifest.digest().as_str())
    }
}

/// The lifecycle phase a catalog job observes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPhase {
    /// Provision the pinned digest from the install plan.
    Install,
    /// Move an installed service to a newer verified release.
    Upgrade,
    /// Capture a pre-upgrade or scheduled backup.
    Backup,
    /// Run the declared health probe.
    Health,
    /// Restore the previous verified release.
    Rollback,
    /// Remove the service and its data per the declared effects.
    Remove,
}

impl JobPhase {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            JobPhase::Install => "install",
            JobPhase::Upgrade => "upgrade",
            JobPhase::Backup => "backup",
            JobPhase::Health => "health",
            JobPhase::Rollback => "rollback",
            JobPhase::Remove => "remove",
        }
    }
}

/// The outcome state of a catalog job. `Succeeded` and `RolledBack`
/// are terminal; `Failed` is retryable while attempts remain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobOutcome {
    /// Created, not yet started.
    Pending,
    /// Running a provisioning attempt.
    Running,
    /// Completed successfully.
    Succeeded,
    /// The current attempt failed; retries may remain.
    Failed,
    /// Failed and restored to the previous verified release.
    RolledBack,
}

impl JobOutcome {
    /// Whether no further transition is legal.
    pub fn is_terminal(self) -> bool {
        matches!(self, JobOutcome::Succeeded | JobOutcome::RolledBack)
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            JobOutcome::Pending => "pending",
            JobOutcome::Running => "running",
            JobOutcome::Succeeded => "succeeded",
            JobOutcome::Failed => "failed",
            JobOutcome::RolledBack => "rolled_back",
        }
    }
}

/// An observable lifecycle job with bounded retries, a redacted
/// last error, and operator guidance on rollback. Jobs are the
/// audit anchor for every catalog mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceJob {
    id: Uuid,
    entry_id: String,
    phase: JobPhase,
    outcome: JobOutcome,
    attempts: u32,
    max_attempts: u32,
    last_error: Option<String>,
    guidance: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ServiceJob {
    /// Begin a pending job for `entry_id` in `phase` with a bounded
    /// retry ceiling.
    pub fn begin(
        id: Uuid,
        entry_id: impl Into<String>,
        phase: JobPhase,
        max_attempts: u32,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            entry_id: entry_id.into(),
            phase,
            outcome: JobOutcome::Pending,
            attempts: 0,
            max_attempts,
            last_error: None,
            guidance: None,
            created_at: at,
            updated_at: at,
        }
    }

    /// Start (or retry) an attempt. Consumes one attempt budget.
    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), VerifiedServiceCatalogError> {
        self.transition_to(JobOutcome::Running, at)?;
        self.attempts += 1;
        Ok(())
    }

    /// Record a successful run.
    pub fn succeed(&mut self, at: DateTime<Utc>) -> Result<(), VerifiedServiceCatalogError> {
        self.transition_to(JobOutcome::Succeeded, at)
    }

    /// Record a failed attempt. The error is redacted through the
    /// canonical deployment-adapter diagnostic contract.
    pub fn fail(
        &mut self,
        at: DateTime<Utc>,
        error: &str,
    ) -> Result<(), VerifiedServiceCatalogError> {
        self.transition_to(JobOutcome::Failed, at)?;
        self.last_error = Some(redact_diagnostic(error));
        Ok(())
    }

    /// Roll back after a failure, recording operator guidance.
    pub fn roll_back(
        &mut self,
        at: DateTime<Utc>,
        guidance: impl Into<String>,
    ) -> Result<(), VerifiedServiceCatalogError> {
        self.transition_to(JobOutcome::RolledBack, at)?;
        self.guidance = Some(guidance.into());
        Ok(())
    }

    fn transition_to(
        &mut self,
        to: JobOutcome,
        at: DateTime<Utc>,
    ) -> Result<(), VerifiedServiceCatalogError> {
        if !super::logic::service_job_transition_allowed(self.outcome, to) {
            return Err(VerifiedServiceCatalogError::Invalid(format!(
                "illegal job transition {} -> {}",
                self.outcome.as_str(),
                to.as_str()
            )));
        }
        self.outcome = to;
        self.updated_at = at;
        Ok(())
    }

    /// Job id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Entry the job operates on.
    pub fn entry_id(&self) -> &str {
        &self.entry_id
    }

    /// Lifecycle phase.
    pub fn phase(&self) -> JobPhase {
        self.phase
    }

    /// Current outcome.
    pub fn outcome(&self) -> JobOutcome {
        self.outcome
    }

    /// Attempts consumed so far.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Bounded retry ceiling.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Redacted last error, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Operator guidance recorded on rollback, if any.
    pub fn guidance(&self) -> Option<&str> {
        self.guidance.as_deref()
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Last transition timestamp.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

//! Verified-service-catalog service: signature-verified entry
//! submission, reviewable install planning, bounded-retry
//! lifecycle jobs, trust revocation, and audit fan-out.
//!
//! The service is the single entry point that the review surfaces,
//! the API, the CLI, and the integration tests share. It owns:
//! - the trusted publisher registry (curated identities),
//! - the catalog entries keyed by stable id, each with its trust
//!   state and installed/previous release plans,
//! - the active [`RevocationList`] consulted at request time (a
//!   revoked entry is refused even when cached),
//! - the observable [`ServiceJob`] history,
//! - the audit fan-out (`openpanel_core::AuditService`), routed
//!   through the canonical `redact_metadata` allowlist.
//!
//! Provisioning is delegated to the [`CatalogProvisioner`] port;
//! the app wiring points it at the existing software-center install
//! wizard and container-runtime services. Templates cannot execute
//! opaque host scripts: the port only accepts reviewed
//! [`ServicePlan`] values.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService, audit::redact_metadata};
use openpanel_domain::{
    JobOutcome, JobPhase, Role, User,
    verified_service_catalog::{
        CatalogProvisioner, HostPolicy, PublisherIdentity, RevocationList, ServiceJob,
        ServiceManifest, ServicePlan, TrustState, VerifiedServiceCatalogError, can_retry,
        check_target_compatibility, ensure_installable, plan_service_install,
        verify_manifest_signature,
    },
};
use serde_json::json;
use uuid::Uuid;

use super::types::{CatalogEntryView, ServiceJobView, VerifiedServiceCatalogServiceError};

/// One catalog entry plus its lifecycle projection. The entry
/// remains cached (`manifest` is retained) even when revoked or
/// blocked — the refusal happens at request time.
struct ManagedEntry {
    manifest: ServiceManifest,
    trust: TrustState,
    installed_plan: Option<ServicePlan>,
    previous_plan: Option<ServicePlan>,
    backup_taken: bool,
}

impl ManagedEntry {
    fn view(&self) -> CatalogEntryView {
        CatalogEntryView {
            id: self.manifest.id().to_owned(),
            name: self.manifest.name().to_owned(),
            version: self.manifest.version().to_owned(),
            publisher_id: self.manifest.publisher().id().to_owned(),
            trust_state: self.trust.as_str().to_owned(),
            installed: self.installed_plan.is_some(),
            installed_digest: self
                .installed_plan
                .as_ref()
                .map(|plan| plan.digest().to_owned()),
        }
    }
}

/// State-of-the-world held by the service. The `Mutex` is required
/// because the service is shared across HTTP handlers and CLI
/// subcommands.
struct ServiceState {
    entries: BTreeMap<String, ManagedEntry>,
    publishers: BTreeMap<String, PublisherIdentity>,
    revocations: RevocationList,
    jobs: BTreeMap<Uuid, ServiceJob>,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            publishers: BTreeMap::new(),
            revocations: RevocationList::empty(),
            jobs: BTreeMap::new(),
        }
    }
}

/// The verified service catalog. Composition root wires the
/// provisioner port (software-center / container-runtime) and the
/// audit sink into it.
pub struct VerifiedServiceCatalogService {
    provisioner: Arc<dyn CatalogProvisioner>,
    audit: Arc<dyn AuditService>,
    state: Mutex<ServiceState>,
}

impl VerifiedServiceCatalogService {
    /// Compose a service with a provisioner port and an audit sink.
    pub fn new(provisioner: Arc<dyn CatalogProvisioner>, audit: Arc<dyn AuditService>) -> Self {
        Self {
            provisioner,
            audit,
            state: Mutex::new(ServiceState::default()),
        }
    }

    /// Owner-only gate for the curated trust registry (adding and
    /// revoking publishers is a governance act).
    fn require_owner(user: &User) -> Result<(), VerifiedServiceCatalogServiceError> {
        if matches!(user.role(), Role::Owner) {
            Ok(())
        } else {
            Err(VerifiedServiceCatalogServiceError::Forbidden)
        }
    }

    /// Owner/Admin gate (defense in depth; handlers gate again).
    fn require_operator(user: &User) -> Result<(), VerifiedServiceCatalogServiceError> {
        if matches!(user.role(), Role::Owner | Role::Admin) {
            Ok(())
        } else {
            Err(VerifiedServiceCatalogServiceError::Forbidden)
        }
    }

    // -----------------------------------------------------------------
    // Trust registry
    // -----------------------------------------------------------------

    /// Add a curated publisher identity. Only verified publishers
    /// contribute the `Verified` trust tier; other correctly-signed
    /// manifests join as `Community`.
    pub fn trust_publisher(
        &self,
        caller: &User,
        publisher: PublisherIdentity,
    ) -> Result<(), VerifiedServiceCatalogServiceError> {
        Self::require_owner(caller).inspect_err(|_err| {
            self.audit_denied(caller, "trust_publisher", publisher.id());
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        state
            .publishers
            .insert(publisher.id().to_owned(), publisher.clone());
        drop(state);
        self.audit_event(
            caller,
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
            publisher.id(),
            json!({
                "intent": "trust_publisher",
                "publisher": publisher.id(),
            }),
        );
        Ok(())
    }

    /// Revoke a publisher. Every entry it signed becomes
    /// non-installable at request time, even when cached locally;
    /// the refusal is audited without credentials.
    pub fn revoke_publisher(
        &self,
        caller: &User,
        publisher_id: &str,
    ) -> Result<(), VerifiedServiceCatalogServiceError> {
        Self::require_owner(caller).inspect_err(|_err| {
            self.audit_denied(caller, "revoke_publisher", publisher_id);
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        if !state.publishers.contains_key(publisher_id)
            && !state
                .entries
                .values()
                .any(|entry| entry.manifest.publisher().id() == publisher_id)
        {
            return Err(VerifiedServiceCatalogServiceError::PublisherNotFound(
                publisher_id.to_owned(),
            ));
        }
        state.revocations.revoke_publisher_id(publisher_id);
        for entry in state.entries.values_mut() {
            if entry.manifest.publisher().id() == publisher_id {
                entry.trust = TrustState::Revoked;
            }
        }
        drop(state);
        self.audit_event(
            caller,
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
            publisher_id,
            json!({
                "intent": "revoke_publisher",
                "publisher": publisher_id,
            }),
        );
        Ok(())
    }

    /// Revoke a single artifact by its pinned source digest.
    pub fn revoke_digest(
        &self,
        caller: &User,
        digest: &str,
    ) -> Result<(), VerifiedServiceCatalogServiceError> {
        Self::require_owner(caller).inspect_err(|_err| {
            self.audit_denied(caller, "revoke_digest", digest);
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        let known = state
            .entries
            .values()
            .any(|entry| entry.manifest.digest().as_str() == digest);
        if !known {
            return Err(VerifiedServiceCatalogServiceError::EntryNotFound(format!(
                "no catalog entry publishes digest {digest}"
            )));
        }
        state.revocations.revoke_artifact_digest(digest);
        for entry in state.entries.values_mut() {
            if entry.manifest.digest().as_str() == digest {
                entry.trust = TrustState::Revoked;
            }
        }
        drop(state);
        self.audit_event(
            caller,
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
            digest,
            json!({
                "intent": "revoke_digest",
                "digest": digest,
            }),
        );
        Ok(())
    }

    // -----------------------------------------------------------------
    // Entry review
    // -----------------------------------------------------------------

    /// Submit a manifest for catalog review. Signature verification
    /// happens before the entry can ever be planned; a manifest
    /// whose signature does not verify is recorded with trust
    /// `Blocked` (kept for review evidence, refused for planning and
    /// install).
    pub fn submit_entry(
        &self,
        caller: &User,
        manifest: ServiceManifest,
    ) -> Result<CatalogEntryView, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "submit_entry", manifest.id());
        })?;
        let trusted: Vec<PublisherIdentity> = {
            let state = self.state.lock().map_err(internal)?;
            state.publishers.values().cloned().collect()
        };
        let trust = match verify_manifest_signature(&manifest, &trusted) {
            Ok(trust) => trust,
            Err(VerifiedServiceCatalogError::InvalidSignature) => TrustState::Blocked,
            Err(other) => return Err(other.into()),
        };
        let mut state = self.state.lock().map_err(internal)?;
        if state.entries.contains_key(manifest.id()) {
            return Err(VerifiedServiceCatalogServiceError::Domain(
                VerifiedServiceCatalogError::Invalid(format!(
                    "catalog entry `{}` already exists",
                    manifest.id()
                )),
            ));
        }
        let entry = ManagedEntry {
            manifest: manifest.clone(),
            trust,
            installed_plan: None,
            previous_plan: None,
            backup_taken: false,
        };
        let view = entry.view();
        state.entries.insert(manifest.id().to_owned(), entry);
        drop(state);
        self.audit_event(
            caller,
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
            view.id.clone(),
            json!({
                "intent": "submit_entry",
                "publisher": view.publisher_id,
                "trust_state": view.trust_state,
                "version": view.version,
            }),
        );
        Ok(view)
    }

    /// Projected list of catalog entries, sorted by id.
    pub fn entries(
        &self,
        caller: &User,
    ) -> Result<Vec<CatalogEntryView>, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller)?;
        let state = self.state.lock().map_err(internal)?;
        Ok(state.entries.values().map(ManagedEntry::view).collect())
    }

    /// One projected catalog entry.
    pub fn entry(
        &self,
        caller: &User,
        entry_id: &str,
    ) -> Result<CatalogEntryView, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller)?;
        let state = self.state.lock().map_err(internal)?;
        let entry = self.entry_ref(&state, entry_id)?;
        Ok(entry.view())
    }

    // -----------------------------------------------------------------
    // Reviewable install plan
    // -----------------------------------------------------------------

    /// Build the reviewable install preview for `entry_id`. The
    /// entry must pass the installability gate (trust + revocation)
    /// and target compatibility; planning then either returns the
    /// effective resource/permission preview or the first specific
    /// [`openpanel_domain::PlanConflict`] — performing no mutation.
    pub fn plan_install(
        &self,
        caller: &User,
        entry_id: &str,
        host_policy: &HostPolicy,
    ) -> Result<ServicePlan, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "plan_install", entry_id);
        })?;
        let (manifest, trust, revocations) = {
            let state = self.state.lock().map_err(internal)?;
            let entry = self.entry_ref(&state, entry_id)?;
            (
                entry.manifest.clone(),
                entry.trust,
                state.revocations.clone(),
            )
        };
        ensure_installable(trust, &manifest, &revocations)
            .inspect_err(|_err| self.audit_refused(caller, "plan_install", &manifest))?;
        check_target_compatibility(&manifest, host_policy)?;
        Ok(plan_service_install(&manifest, host_policy)?)
    }

    // -----------------------------------------------------------------
    // Lifecycle jobs
    // -----------------------------------------------------------------

    /// Install the entry's pinned digest behind a reviewed plan.
    /// Every attempt is an observable job with bounded retries and
    /// an audit event on the terminal outcome.
    pub fn install_entry(
        &self,
        caller: &User,
        entry_id: &str,
        host_policy: &HostPolicy,
    ) -> Result<ServiceJobView, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "install", entry_id);
        })?;
        let (manifest, trust, revocations) = {
            let state = self.state.lock().map_err(internal)?;
            let entry = self.entry_ref(&state, entry_id)?;
            (
                entry.manifest.clone(),
                entry.trust,
                state.revocations.clone(),
            )
        };
        ensure_installable(trust, &manifest, &revocations)
            .inspect_err(|_err| self.audit_refused(caller, "install", &manifest))?;
        check_target_compatibility(&manifest, host_policy)?;
        let plan = plan_service_install(&manifest, host_policy)?;
        let job = self.run_job(caller, &manifest, JobPhase::Install, |svc| {
            svc.provisioner
                .install(&plan)
                .map(|()| svc.probe_health(&manifest, entry_id))
        })?;
        if job.outcome() == JobOutcome::Succeeded {
            let mut state = self.state.lock().map_err(internal)?;
            if let Some(entry) = state.entries.get_mut(entry_id) {
                entry.installed_plan = Some(plan);
                entry.backup_taken = false;
            }
            drop(state);
        }
        Ok(service_job_view(&job))
    }

    /// Upgrade an installed entry to a newer signed manifest. A
    /// failed health verification restores the previous verified
    /// release when the upgrade policy requires rollback, and the
    /// job records operator guidance.
    pub fn upgrade_entry(
        &self,
        caller: &User,
        manifest: &ServiceManifest,
        host_policy: &HostPolicy,
    ) -> Result<ServiceJobView, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "upgrade_entry", manifest.id());
        })?;
        let (previous_plan, backup_taken, revocations, trusted) = {
            let state = self.state.lock().map_err(internal)?;
            let entry = self.entry_ref(&state, manifest.id())?;
            (
                entry.installed_plan.clone(),
                entry.backup_taken,
                state.revocations.clone(),
                state.publishers.values().cloned().collect::<Vec<_>>(),
            )
        };
        // Re-verify the new release's signature before touching the
        // current installation.
        let new_trust = verify_manifest_signature(manifest, &trusted)?;
        ensure_installable(new_trust, manifest, &revocations)
            .inspect_err(|_err| self.audit_refused(caller, "upgrade_entry", manifest))?;
        let previous = previous_plan.ok_or(VerifiedServiceCatalogServiceError::Domain(
            VerifiedServiceCatalogError::Invalid(format!(
                "entry `{}` is not installed; run install first",
                manifest.id()
            )),
        ))?;
        if manifest.backup_policy().required_before_upgrade() && !backup_taken {
            return Err(VerifiedServiceCatalogServiceError::Domain(
                VerifiedServiceCatalogError::Invalid(format!(
                    "entry `{}` requires a successful backup before upgrade",
                    manifest.id()
                )),
            ));
        }
        check_target_compatibility(manifest, host_policy)?;
        let plan = plan_service_install(manifest, host_policy)?;
        let entry_id = manifest.id().to_owned();
        let rollback_required = manifest.upgrade_policy().rollback_on_failure();
        let mut job = self.run_job(caller, manifest, JobPhase::Upgrade, |svc| {
            svc.provisioner
                .install(&plan)
                .map(|()| svc.probe_health(manifest, &entry_id))
        })?;
        if job.outcome() == JobOutcome::Succeeded {
            let mut state = self.state.lock().map_err(internal)?;
            if let Some(entry) = state.entries.get_mut(&entry_id) {
                entry.manifest = manifest.clone();
                entry.installed_plan = Some(plan);
                entry.previous_plan = Some(previous);
                entry.backup_taken = false;
                entry.trust = new_trust;
            }
        } else if rollback_required {
            // Restore the previous verified release and record the
            // rollback on the job with operator guidance.
            let restored = matches!(self.provisioner.install(&previous), Ok(()))
                && self.probe_health(manifest, &entry_id);
            let guidance = format!(
                "previous verified release {} restored {}; fix the failing health probe and retry",
                previous.digest(),
                if restored {
                    "successfully"
                } else {
                    "with errors"
                },
            );
            let _ = job.roll_back(Utc::now(), guidance.clone());
            let mut state = self.state.lock().map_err(internal)?;
            if let Some(entry) = state.entries.get_mut(&entry_id) {
                entry.installed_plan = Some(previous.clone());
                entry.previous_plan = None;
            }
            state.jobs.insert(job.id(), job.clone());
            drop(state);
            self.audit_event(
                caller,
                AuditAction::SoftwareChanged,
                AuditOutcome::Failure,
                entry_id.clone(),
                json!({
                    "intent": "upgrade_rollback",
                    "phase": JobPhase::Upgrade.as_str(),
                    "state": JobOutcome::RolledBack.as_str(),
                    "digest": plan.digest(),
                    "restored_digest": previous.digest(),
                    "guidance": guidance,
                }),
            );
        }
        Ok(service_job_view(&job))
    }

    /// Capture a backup for the installed entry. The backup is an
    /// observable job; success clears the pre-upgrade requirement.
    pub fn run_backup(
        &self,
        caller: &User,
        entry_id: &str,
    ) -> Result<ServiceJobView, VerifiedServiceCatalogServiceError> {
        let job = self.lifecycle_job(caller, entry_id, JobPhase::Backup, |svc, manifest| {
            let id = manifest.id().to_owned();
            svc.provisioner.backup(&id)
        })?;
        if job.outcome() == JobOutcome::Succeeded {
            let mut state = self.state.lock().map_err(internal)?;
            if let Some(entry) = state.entries.get_mut(entry_id) {
                entry.backup_taken = true;
            }
        }
        Ok(service_job_view(&job))
    }

    /// Run the declared health probe as an observable job.
    pub fn verify_health(
        &self,
        caller: &User,
        entry_id: &str,
    ) -> Result<ServiceJobView, VerifiedServiceCatalogServiceError> {
        let job = self.lifecycle_job(caller, entry_id, JobPhase::Health, |svc, manifest| {
            let id = manifest.id().to_owned();
            if svc.probe_health(manifest, &id) {
                Ok(())
            } else {
                Err(VerifiedServiceCatalogError::Provision(
                    "health probe failed".into(),
                ))
            }
        })?;
        Ok(service_job_view(&job))
    }

    /// Restore the previous verified release after a failed
    /// upgrade that did not carry its own rollback.
    pub fn rollback(
        &self,
        caller: &User,
        entry_id: &str,
    ) -> Result<ServiceJobView, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "rollback", entry_id);
        })?;
        let (previous, manifest) =
            {
                let state = self.state.lock().map_err(internal)?;
                let entry = self.entry_ref(&state, entry_id)?;
                let previous = entry.previous_plan.clone().ok_or(
                    VerifiedServiceCatalogServiceError::Domain(
                        VerifiedServiceCatalogError::Invalid(format!(
                            "entry `{entry_id}` has no previous verified release to restore"
                        )),
                    ),
                )?;
                (previous, entry.manifest.clone())
            };
        let id = entry_id.to_owned();
        let job = self.run_job(caller, &manifest, JobPhase::Rollback, |svc| {
            svc.provisioner
                .install(&previous)
                .map(|()| svc.probe_health(&manifest, &id))
        })?;
        if job.outcome() == JobOutcome::Succeeded {
            let mut state = self.state.lock().map_err(internal)?;
            if let Some(entry) = state.entries.get_mut(entry_id) {
                entry.installed_plan = Some(previous);
                entry.previous_plan = None;
            }
        }
        Ok(service_job_view(&job))
    }

    /// Remove the service and its declared data effects. The
    /// catalog keeps the (uninstalled) manifest for review evidence.
    pub fn remove_entry(
        &self,
        caller: &User,
        entry_id: &str,
    ) -> Result<ServiceJobView, VerifiedServiceCatalogServiceError> {
        let job = self.lifecycle_job(caller, entry_id, JobPhase::Remove, |svc, manifest| {
            let id = manifest.id().to_owned();
            svc.provisioner.remove(&id)
        })?;
        if job.outcome() == JobOutcome::Succeeded {
            let mut state = self.state.lock().map_err(internal)?;
            if let Some(entry) = state.entries.get_mut(entry_id) {
                entry.installed_plan = None;
                entry.previous_plan = None;
                entry.backup_taken = false;
            }
        }
        Ok(service_job_view(&job))
    }

    /// Projected job history, newest first.
    pub fn jobs(
        &self,
        caller: &User,
    ) -> Result<Vec<ServiceJobView>, VerifiedServiceCatalogServiceError> {
        Self::require_operator(caller)?;
        let state = self.state.lock().map_err(internal)?;
        let mut jobs: Vec<ServiceJob> = state.jobs.values().cloned().collect();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at()));
        Ok(jobs.into_iter().map(|job| service_job_view(&job)).collect())
    }

    // -----------------------------------------------------------------
    // Shared helpers
    // -----------------------------------------------------------------

    fn entry_ref<'a>(
        &self,
        state: &'a ServiceState,
        entry_id: &str,
    ) -> Result<&'a ManagedEntry, VerifiedServiceCatalogServiceError> {
        state
            .entries
            .get(entry_id)
            .ok_or_else(|| VerifiedServiceCatalogServiceError::EntryNotFound(entry_id.to_owned()))
    }

    /// One-shot bounded lifecycle job around a provisioner call.
    fn lifecycle_job<F>(
        &self,
        caller: &User,
        entry_id: &str,
        phase: JobPhase,
        work: F,
    ) -> Result<ServiceJob, VerifiedServiceCatalogServiceError>
    where
        F: Fn(&Self, &ServiceManifest) -> Result<(), VerifiedServiceCatalogError>,
    {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, phase.as_str(), entry_id);
        })?;
        let (manifest, trust, revocations) = {
            let state = self.state.lock().map_err(internal)?;
            let entry = self.entry_ref(&state, entry_id)?;
            (
                entry.manifest.clone(),
                entry.trust,
                state.revocations.clone(),
            )
        };
        ensure_installable(trust, &manifest, &revocations)
            .inspect_err(|_err| self.audit_refused(caller, phase.as_str(), &manifest))?;
        self.run_job(caller, &manifest, phase, |svc| {
            work(svc, &manifest).map(|()| true)
        })
    }

    /// Drive one observable job with bounded retries and the audit
    /// fan-out for its terminal outcome. `attempt` performs one
    /// provisioning attempt and reports whether health passed.
    fn run_job<A>(
        &self,
        caller: &User,
        manifest: &ServiceManifest,
        phase: JobPhase,
        attempt: A,
    ) -> Result<ServiceJob, VerifiedServiceCatalogServiceError>
    where
        A: Fn(&Self) -> Result<bool, VerifiedServiceCatalogError>,
    {
        let max_attempts = manifest.upgrade_policy().max_attempts();
        let mut job = ServiceJob::begin(
            Uuid::new_v4(),
            manifest.id(),
            phase,
            max_attempts,
            Utc::now(),
        );
        loop {
            job.start(Utc::now())
                .map_err(|err| internal(err.to_string()))?;
            match attempt(self) {
                Ok(true) => {
                    job.succeed(Utc::now())
                        .map_err(|err| internal(err.to_string()))?;
                    break;
                }
                Ok(false) => {
                    job.fail(Utc::now(), "health probe failed")
                        .map_err(|err| internal(err.to_string()))?;
                }
                Err(error) => {
                    job.fail(Utc::now(), &error.to_string())
                        .map_err(|err| internal(err.to_string()))?;
                }
            }
            if !can_retry(&job) {
                break;
            }
        }
        let mut state = self.state.lock().map_err(internal)?;
        state.jobs.insert(job.id(), job.clone());
        drop(state);
        let outcome = match job.outcome() {
            JobOutcome::Succeeded => AuditOutcome::Success,
            _ => AuditOutcome::Failure,
        };
        self.audit_event(
            caller,
            AuditAction::SoftwareChanged,
            outcome,
            job.entry_id().to_owned(),
            json!({
                "intent": "lifecycle_job",
                "phase": job.phase().as_str(),
                "job_id": job.id().to_string(),
                "state": job.outcome().as_str(),
                "attempts": job.attempts(),
                "diagnostic": job.last_error().unwrap_or_default(),
                "guidance": job.guidance().unwrap_or_default(),
                "digest": manifest.digest().as_str(),
            }),
        );
        Ok(job)
    }

    /// Probe health up to the declared bounded attempts.
    fn probe_health(&self, manifest: &ServiceManifest, entry_id: &str) -> bool {
        for _ in 0..manifest.health().max_attempts() {
            if matches!(self.provisioner.health_check(entry_id), Ok(outcome) if outcome.healthy) {
                return true;
            }
        }
        false
    }

    // -----------------------------------------------------------------
    // Audit fan-out
    // -----------------------------------------------------------------

    fn audit_event(
        &self,
        caller: &User,
        action: AuditAction,
        outcome: AuditOutcome,
        target: impl Into<String>,
        metadata: serde_json::Value,
    ) {
        let event = AuditEvent::new(caller.username().as_str(), action, outcome)
            .target(target)
            .metadata(redact_metadata(&metadata));
        let audit = self.audit.clone();
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }

    fn audit_denied(&self, caller: &User, intent: &str, target: &str) {
        self.audit_event(
            caller,
            AuditAction::PermissionDenied,
            AuditOutcome::Denied,
            target.to_owned(),
            json!({ "intent": intent }),
        );
    }

    /// Refusals (blocked / revoked entries) are audited without
    /// exposing credentials or manifest secret material — the
    /// metadata carries ids and wire names only, and the whole
    /// payload passes the canonical `redact_metadata` allowlist.
    fn audit_refused(&self, caller: &User, intent: &str, manifest: &ServiceManifest) {
        self.audit_event(
            caller,
            AuditAction::PermissionDenied,
            AuditOutcome::Denied,
            manifest.id().to_owned(),
            json!({
                "intent": intent,
                "publisher": manifest.publisher().id(),
                "digest": manifest.digest().as_str(),
            }),
        );
    }
}

fn service_job_view(job: &ServiceJob) -> ServiceJobView {
    ServiceJobView {
        id: job.id(),
        entry_id: job.entry_id().to_owned(),
        phase: job.phase().as_str().to_owned(),
        outcome: job.outcome().as_str().to_owned(),
        attempts: job.attempts(),
        max_attempts: job.max_attempts(),
        last_error: job.last_error().map(str::to_owned),
        guidance: job.guidance().map(str::to_owned),
    }
}

fn internal<E: std::fmt::Display>(err: E) -> VerifiedServiceCatalogServiceError {
    VerifiedServiceCatalogServiceError::Domain(VerifiedServiceCatalogError::Invalid(format!(
        "internal: {err}"
    )))
}

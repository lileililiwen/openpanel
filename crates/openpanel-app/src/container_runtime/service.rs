//! Container runtime application service: per-user quota,
//! registry credentials, metrics, and monthly egress accounting.
//!
//! The bounded context owns the persistence of these rows and
//! exposes a single quota gate the docker service calls before
//! every container create. The pull flow uses an injected
//! `RegistryHostAdapter` so production wires a real `bollard`
//! adapter and tests wire the no-op.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::audit::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    ContainerMetrics, ContainerQuota, ContainerRuntimeError, ContainerRuntimeRepository,
    EffectiveQuota, PlanQuotaCaps, QuotaAxis, RegistryCredential, RegistryCredentialId, RepoError,
    Role, User, check_concurrent, check_cpu, check_egress, check_memory, check_total,
    effective_quota, egress_threshold_crossed,
};
use uuid::Uuid;

use super::{
    crypto,
    registry_adapter::{PullError, PullRequest, PullResult, RegistryHostAdapter},
};

/// Minimum registry-password length (the spec's "weak passwords"
/// rule: ≥ 16 chars with mixed classes). We enforce length here;
/// the API/CLI form multiplies further with class checks.
pub const MIN_REGISTRY_PASSWORD_LEN: usize = 16;

/// Request body for a `set_quota` call. Missing fields preserve
/// the previous value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuotaUpdate {
    /// Optional new `max_concurrent`.
    pub max_concurrent: Option<u32>,
    /// Optional new `max_total`.
    pub max_total: Option<u32>,
    /// Optional new `cpu_pct_max`.
    pub cpu_pct_max: Option<u8>,
    /// Optional new `memory_bytes_max`.
    pub memory_bytes_max: Option<u64>,
    /// Optional new `egress_bytes_per_month`.
    pub egress_bytes_per_month: Option<u64>,
}

/// What the docker service (or any other consumer) must know about
/// the user's current container usage to validate a new create.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageSnapshot {
    /// How many containers are presently running.
    pub running_concurrent: u32,
    /// How many containers the user owns in total.
    pub total: u32,
    /// Egress used in `YYYY-MM` so far.
    pub egress_used: u64,
}

/// A passing quota gate result. Includes the effective quota
/// in force (after plan tightening) so the audit can record the
/// axes that were tightened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaDecision {
    /// Effective quota the consumer sees.
    pub effective: ContainerQuota,
    /// Axes the plan tightened below the user-set value.
    pub plan_overrides: Vec<QuotaAxis>,
}

/// Container runtime application service.
pub struct ContainerRuntimeService {
    repo: Arc<dyn ContainerRuntimeRepository>,
    audit: Arc<dyn AuditService>,
    registry: Arc<dyn RegistryHostAdapter>,
    master_key: std::sync::RwLock<[u8; crypto::KEY_LEN]>,
    plan_caps: std::sync::RwLock<PlanQuotaCaps>,
}

impl ContainerRuntimeService {
    /// Build with the supplied dependencies and master key.
    /// `master_key_b64` is the base64-encoded 32-byte AES master
    /// key (matches the format used by the other bounded contexts
    /// and stored in `OPENPANEL__DATABASE__MASTER_KEY`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn ContainerRuntimeRepository>,
        audit: Arc<dyn AuditService>,
        registry: Arc<dyn RegistryHostAdapter>,
        master_key_b64: &str,
        plan_caps: PlanQuotaCaps,
    ) -> Result<Self, ContainerRuntimeError> {
        let master_key = crypto::decode_master_key(master_key_b64)?;
        Ok(Self::from_master_key(
            repo, audit, registry, master_key, plan_caps,
        ))
    }

    /// Build with the already-decoded 32-byte master key. Used
    /// by composition roots that have already called
    /// `db_crypto::decode_master_key` for another module.
    #[allow(clippy::too_many_arguments)]
    pub fn from_master_key(
        repo: Arc<dyn ContainerRuntimeRepository>,
        audit: Arc<dyn AuditService>,
        registry: Arc<dyn RegistryHostAdapter>,
        master_key: [u8; crypto::KEY_LEN],
        plan_caps: PlanQuotaCaps,
    ) -> Self {
        Self {
            repo,
            audit,
            registry,
            master_key: std::sync::RwLock::new(master_key),
            plan_caps: std::sync::RwLock::new(plan_caps),
        }
    }

    /// Replace the master key at runtime (used by the migrate CLI
    /// when rotating the panel master key).
    pub fn rotate_master_key(&self, master_key_b64: &str) -> Result<(), ContainerRuntimeError> {
        let key = crypto::decode_master_key(master_key_b64)?;
        #[allow(clippy::expect_used)]
        // The lock is only poisoned if a previous holder panicked; this service has no panic paths.
        let mut guard = self.master_key.write().expect("invariant: rwlock poisoned");
        *guard = key;
        Ok(())
    }

    /// Replace the plan-imposed caps (e.g. when a user upgrades their plan).
    pub fn set_plan_caps(&self, caps: PlanQuotaCaps) {
        #[allow(clippy::expect_used)]
        // The lock is only poisoned if a previous holder panicked; this service has no panic paths.
        let mut guard = self.plan_caps.write().expect("invariant: rwlock poisoned");
        *guard = caps;
    }

    /// Snapshot the plan-imposed caps.
    pub fn plan_caps(&self) -> PlanQuotaCaps {
        #[allow(clippy::expect_used)]
        // The lock is only poisoned if a previous holder panicked; this service has no panic paths.
        let caps = self
            .plan_caps
            .read()
            .expect("invariant: rwlock poisoned")
            .clone();
        caps
    }

    /// Read the per-user quota. If absent in storage, defaults are
    /// returned (no implicit insert — the row is materialised on
    /// first explicit `set_quota`).
    pub async fn get_quota(
        &self,
        caller: &User,
        user_id: Uuid,
    ) -> Result<EffectiveQuota, ContainerRuntimeError> {
        self.enforce_quota_read(caller, user_id)?;
        let quota = self.read_or_default(user_id).await?;
        let caps = self.plan_caps();
        Ok(effective_quota(&quota, &caps))
    }

    /// Update the per-user quota. Owner/Admin only; missing
    /// fields preserve the previous value. The plan-imposed caps
    /// tighten the result; the audit records
    /// `ContainerQuotaPlanOverride` per tightened axis.
    pub async fn set_quota(
        &self,
        caller: &User,
        user_id: Uuid,
        update: QuotaUpdate,
    ) -> Result<EffectiveQuota, ContainerRuntimeError> {
        if !matches!(caller.role(), Role::Owner | Role::Admin) {
            return Err(ContainerRuntimeError::Forbidden);
        }
        let previous = self.read_or_default(user_id).await?;
        let mut quota = previous.clone();
        if let Some(v) = update.max_concurrent {
            quota.max_concurrent = v;
        }
        if let Some(v) = update.max_total {
            quota.max_total = v;
        }
        if let Some(v) = update.cpu_pct_max {
            quota.cpu_pct_max = v;
        }
        if let Some(v) = update.memory_bytes_max {
            quota.memory_bytes_max = v;
        }
        if let Some(v) = update.egress_bytes_per_month {
            quota.egress_bytes_per_month = v;
        }
        quota.updated_at = Utc::now();
        quota.validate()?;
        self.repo.save_quota(&quota).await?;
        let caps = self.plan_caps();
        let effective = effective_quota(&quota, &caps);
        for axis in &effective.plan_overrides {
            self.audit_quota_override(caller, *axis).await;
        }
        Ok(effective)
    }

    /// The single quota gate the docker service MUST call before
    /// every container create. Returns the effective quota when
    /// all axes pass; otherwise `Err(QuotaExceeded{axis})` and
    /// the audit `ContainerStartQuotaBlocked` records the axis.
    pub async fn check_quota(
        &self,
        caller: &User,
        user_id: Uuid,
        usage: UsageSnapshot,
        proposed: ProposedContainer,
    ) -> Result<QuotaDecision, ContainerRuntimeError> {
        self.enforce_quota_read(caller, user_id)?;
        let quota = self.read_or_default(user_id).await?;
        let caps = self.plan_caps();
        let effective = effective_quota(&quota, &caps);
        let q = &effective.quota;
        if let Err(e) = check_concurrent(q, usage.running_concurrent, proposed.count) {
            self.audit_quota_block(caller, user_id, e_axis(&e)).await;
            return Err(e);
        }
        if let Err(e) = check_total(q, usage.total, proposed.count) {
            self.audit_quota_block(caller, user_id, e_axis(&e)).await;
            return Err(e);
        }
        if let Err(e) = check_cpu(q, proposed.cpu_pct) {
            self.audit_quota_block(caller, user_id, e_axis(&e)).await;
            return Err(e);
        }
        if let Err(e) = check_memory(q, proposed.memory_bytes) {
            self.audit_quota_block(caller, user_id, e_axis(&e)).await;
            return Err(e);
        }
        if let Err(e) = check_egress(q, usage.egress_used, proposed.egress_delta) {
            self.audit_quota_block(caller, user_id, e_axis(&e)).await;
            return Err(e);
        }
        Ok(QuotaDecision {
            effective: q.clone(),
            plan_overrides: effective.plan_overrides.clone(),
        })
    }

    /// Save a fresh metrics sample. Used by the metrics poller
    /// background task and by the `POST /containers/{id}/metrics`
    /// endpoint.
    pub async fn record_metrics(
        &self,
        caller: &User,
        metrics: ContainerMetrics,
    ) -> Result<(), ContainerRuntimeError> {
        self.enforce_quota_read(caller, metrics.user_id)?;
        if metrics.user_id != caller.id() && !matches!(caller.role(), Role::Owner | Role::Admin) {
            return Err(ContainerRuntimeError::Forbidden);
        }
        self.repo.save_metrics(&metrics).await.map_err(Into::into)
    }

    /// List the latest `limit` metrics samples for a container.
    pub async fn list_metrics(
        &self,
        caller: &User,
        container_id: Uuid,
        limit: u32,
    ) -> Result<Vec<ContainerMetrics>, ContainerRuntimeError> {
        let samples = self.repo.list_metrics(container_id, limit).await?;
        // Authorise: any sample's user_id must match caller or
        // caller is Owner/Admin.
        if let Some(first) = samples.first()
            && first.user_id != caller.id()
            && !matches!(caller.role(), Role::Owner | Role::Admin)
        {
            return Err(ContainerRuntimeError::Forbidden);
        }
        Ok(samples)
    }

    /// Add `delta` bytes to the monthly egress account. The
    /// caller's `user_id` is the account owner; `container_id`
    /// scopes the row when supplied (else `None` = aggregate).
    /// When the new total crosses 80% of `egress_bytes_per_month`
    /// the audit emits a `BandwidthThresholdCrossed`-style event
    /// (spec: Egress Accounting Scenario: Threshold cross).
    pub async fn record_egress(
        &self,
        caller: &User,
        user_id: Uuid,
        container_id: Option<Uuid>,
        delta: u64,
        month: &str,
    ) -> Result<u64, ContainerRuntimeError> {
        self.enforce_quota_read(caller, user_id)?;
        let new_total = self
            .repo
            .add_egress(user_id, container_id, month, delta)
            .await?;
        let q = self.read_or_default(user_id).await?;
        if egress_threshold_crossed(&q, new_total) {
            // Spec: "Egress Accounting Scenario: Threshold cross" —
            // the egress consumer emits `BandwidthThresholdCrossed`
            // and the container is throttled to 1 Mbps until reset.
            // The throttle imposition itself is enforced by the
            // docker bounded context (this bounded context cannot
            // touch the runtime); here we only record the event.
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::BandwidthThresholdCrossed,
                        AuditOutcome::Success,
                    )
                    .target(user_id.to_string())
                    .metadata(serde_json::json!({
                        "container_id": container_id.map(|u| u.to_string()),
                        "month": month,
                        "bytes": new_total,
                        "limit": q.egress_bytes_per_month,
                    })),
                )
                .await;
        }
        Ok(new_total)
    }

    /// Raise a container's monthly egress limit. Owner/Admin only.
    /// Removes any throttle that was applied at the 80% threshold
    /// (spec: Owner raises egress). The audit emits
    /// `ContainerEgressLimitRaised`.
    pub async fn raise_egress_limit(
        &self,
        caller: &User,
        user_id: Uuid,
        bytes_per_month: u64,
    ) -> Result<ContainerQuota, ContainerRuntimeError> {
        if !matches!(caller.role(), Role::Owner | Role::Admin) {
            return Err(ContainerRuntimeError::Forbidden);
        }
        let mut quota = self.read_or_default(user_id).await?;
        quota.egress_bytes_per_month = bytes_per_month.max(1);
        quota.updated_at = Utc::now();
        quota.validate()?;
        self.repo.save_quota(&quota).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::ContainerEgressLimitRaised,
                    AuditOutcome::Success,
                )
                .target(user_id.to_string())
                .metadata(serde_json::json!({
                    "egress_bytes_per_month": quota.egress_bytes_per_month,
                })),
            )
            .await;
        Ok(quota)
    }

    /// Create a registry credential. The plaintext password is
    /// encrypted under the master key before persistence. The
    /// returned credential is the *redacted* view; the plaintext
    /// is returned exactly once in the API/CLI response via the
    /// separate `CreateCredentialResult` envelope.
    pub async fn create_registry_credential(
        &self,
        caller: &User,
        registry: String,
        username: String,
        password: String,
    ) -> Result<CreateCredentialResult, ContainerRuntimeError> {
        validate_password(&password)?;
        #[allow(clippy::expect_used)]
        // The lock is only poisoned if a previous holder panicked; this service has no panic paths.
        let key = *self.master_key.read().expect("invariant: rwlock poisoned");
        let ciphertext = crypto::encrypt_secret(&key, &password)?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let cred = RegistryCredential::new(
            id,
            caller.id(),
            registry.clone(),
            username.clone(),
            ciphertext,
            now,
        );
        self.repo.save_credential(&cred).await?;
        Ok(CreateCredentialResult {
            credential: cred.redacted(),
            // Plaintext is returned exactly once to the caller; the
            // API/CLI surface MUST strip it from any subsequent read.
            plaintext_once: password,
        })
    }

    /// List the caller's (or, for Owner/Admin, anyone's) registry
    /// credentials. Always redacted.
    pub async fn list_registry_credentials(
        &self,
        caller: &User,
        user_id: Uuid,
    ) -> Result<Vec<RegistryCredential>, ContainerRuntimeError> {
        self.enforce_quota_read(caller, user_id)?;
        let creds = self.repo.list_credentials(user_id).await?;
        Ok(creds.into_iter().map(|c| c.redacted()).collect())
    }

    /// Delete a credential.
    pub async fn delete_registry_credential(
        &self,
        caller: &User,
        id: RegistryCredentialId,
    ) -> Result<bool, ContainerRuntimeError> {
        let cred = self
            .repo
            .get_credential_by_id(id)
            .await?
            .ok_or_else(|| ContainerRuntimeError::CredentialNotFound(id.to_string()))?;
        self.enforce_quota_read(caller, cred.user_id)?;
        self.repo.delete_credential(id).await.map_err(Into::into)
    }

    /// Pull an image on behalf of `container_id` using a stored
    /// credential (anonymous pull when `credential_id` is `None`).
    /// Spec: Pull from registry scenario — adapter MUST redact
    /// upstream reasons (handled in the adapter); audit
    /// `ContainerImagePulled` records `{owner_id, container_id,
    /// image_ref, ref_count}` only.
    pub async fn pull_image(
        &self,
        caller: &User,
        container_id: Uuid,
        image_ref: String,
        registry_credential_id: Option<RegistryCredentialId>,
    ) -> Result<PullResult, PullError> {
        // Resolve credentials first so a 401 fails before the
        // adapter is touched.
        let mut username = None;
        let mut password = None;
        if let Some(id) = registry_credential_id {
            let cred = self
                .repo
                .get_credential_by_id(id)
                .await
                .map_err(|e| PullError::Unavailable(e.0))?
                .ok_or(PullError::AuthFailed)?;
            self.enforce_quota_read(caller, cred.user_id)
                .map_err(|_| PullError::AuthFailed)?;
            #[allow(clippy::expect_used)]
            // The lock is only poisoned if a previous holder panicked; this service has no panic paths.
            let key = *self.master_key.read().expect("invariant: rwlock poisoned");
            let plain = crypto::decrypt_secret(&key, &cred.encrypted_secret)
                .map_err(|_| PullError::AuthFailed)?;
            username = Some(cred.username.clone());
            password = Some(plain);
            // Touch last-used-now; ignore failure (audit is the
            // important signal).
            let _ = self.repo.touch_credential(id, Utc::now()).await;
        }
        let request = PullRequest {
            caller: caller.id(),
            container_id,
            image_ref: image_ref.clone(),
            registry_credential_id,
            username,
            password,
        };
        let result = self.registry.pull(request).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::ContainerImagePulled,
                    AuditOutcome::Success,
                )
                .target(container_id.to_string())
                .metadata(serde_json::json!({
                    "owner_id": caller.id().to_string(),
                    "container_id": container_id.to_string(),
                    "image_ref": image_ref,
                    "ref_count": result.ref_count,
                })),
            )
            .await;
        Ok(result)
    }

    async fn read_or_default(
        &self,
        user_id: Uuid,
    ) -> Result<ContainerQuota, ContainerRuntimeError> {
        Ok(self
            .repo
            .get_quota(user_id)
            .await?
            .unwrap_or_else(|| ContainerQuota::default_for(user_id)))
    }

    fn enforce_quota_read(
        &self,
        caller: &User,
        user_id: Uuid,
    ) -> Result<(), ContainerRuntimeError> {
        if caller.id() == user_id {
            return Ok(());
        }
        if matches!(caller.role(), Role::Owner | Role::Admin) {
            return Ok(());
        }
        Err(ContainerRuntimeError::Forbidden)
    }

    async fn audit_quota_block(&self, caller: &User, user_id: Uuid, axis: QuotaAxis) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::ContainerStartQuotaBlocked,
                    AuditOutcome::Denied,
                )
                .target(user_id.to_string())
                .metadata(serde_json::json!({ "axis": axis })),
            )
            .await;
    }

    async fn audit_quota_override(&self, caller: &User, axis: QuotaAxis) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::ContainerQuotaPlanOverride,
                    AuditOutcome::Success,
                )
                .metadata(serde_json::json!({ "axis": axis })),
            )
            .await;
    }
}

/// What the quota gate must know about the container being
/// created (`proposed`) to validate every axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProposedContainer {
    /// Number of containers this create will spawn (usually 1).
    pub count: u32,
    /// CPU percent reserved (`0..=100`).
    pub cpu_pct: u8,
    /// Memory in bytes.
    pub memory_bytes: u64,
    /// Egress bytes this container is expected to add this month
    /// (typically 0 at create time; used to protect against
    /// account-overflow at the moment of create).
    pub egress_delta: u64,
}

/// One-shot result of a credential create: the redacted row +
/// the plaintext password, returned exactly once.
#[derive(Debug, Clone)]
pub struct CreateCredentialResult {
    /// The redacted credential row (with `encrypted_secret == "-"`).
    pub credential: RegistryCredential,
    /// Plaintext password, returned exactly once. The caller (API
    /// handler) MUST never persist or echo this back in a subsequent
    /// read.
    pub plaintext_once: String,
}

fn validate_password(p: &str) -> Result<(), ContainerRuntimeError> {
    if p.len() < MIN_REGISTRY_PASSWORD_LEN {
        return Err(ContainerRuntimeError::CredentialDecode);
    }
    let has_lower = p.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = p.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = p.chars().any(|c| c.is_ascii_digit());
    let has_punct = p.chars().any(|c| !c.is_alphanumeric() && c.is_ascii());
    if !(has_lower && has_upper && has_digit) {
        return Err(ContainerRuntimeError::CredentialDecode);
    }
    if !has_punct {
        return Err(ContainerRuntimeError::CredentialDecode);
    }
    Ok(())
}

fn e_axis(err: &ContainerRuntimeError) -> QuotaAxis {
    match err {
        ContainerRuntimeError::QuotaExceeded { axis } => *axis,
        _ => QuotaAxis::Total,
    }
}

impl From<RepoError> for PullError {
    fn from(e: RepoError) -> Self {
        PullError::Unavailable(e.0)
    }
}

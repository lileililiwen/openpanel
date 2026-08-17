//! Container runtime bounded context: per-user container quota,
//! registry credentials (stored as opaque ciphertext by the app
//! layer), per-container metrics, and monthly network egress
//! accounting.
//!
//! This module is intentionally zero-I/O: entity construction and
//! invariant validation only. Encryption, persistence, and the
//! docker adapter live in `openpanel_app::container_runtime`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Maximum number of containers a user may own by default.
pub const MAX_CONTAINERS_PER_USER: u32 = 8;
/// Maximum total disk usage (bytes) a user may consume by default.
pub const MAX_DISK_BYTES_PER_USER: u64 = 50 * 1024 * 1024 * 1024;
/// Default monthly egress allowance (100 GiB).
pub const MAX_EGRESS_BYTES_PER_MONTH: u64 = 100 * 1024 * 1024 * 1024;
/// Quota axis that triggered a `QuotaExceeded` rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaAxis {
    /// `max_concurrent` exceeded.
    Concurrent,
    /// `max_total` (own containers) exceeded.
    Total,
    /// `cpu_pct_max` exceeded.
    Cpu,
    /// `memory_bytes_max` exceeded.
    Memory,
    /// `egress_bytes_per_month` exceeded.
    Egress,
}

/// Errors raised by the container runtime bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContainerRuntimeError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The user has reached a quota axis.
    #[error("quota exceeded on {axis:?} axis")]
    QuotaExceeded {
        /// Quota axis that was exceeded.
        axis: QuotaAxis,
    },
    /// The credential password failed decoding after decryption
    /// (UTF-8 or length).
    #[error("credential decode failed")]
    CredentialDecode,
    /// The stored ciphertext is malformed (bad layout, wrong
    /// nonce length, GCM tag mismatch, or empty payload).
    #[error("invalid cipher: {0}")]
    InvalidCipher(String),
    /// The host adapter (docker / registry) is unavailable.
    #[error("adapter unavailable: {0}")]
    AdapterUnavailable(String),
    /// The named credential was not found.
    #[error("credential not found: {0}")]
    CredentialNotFound(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<RepoError> for ContainerRuntimeError {
    fn from(error: RepoError) -> Self {
        ContainerRuntimeError::Persistence(error.0)
    }
}

impl From<ContainerRuntimeError> for RepoError {
    fn from(error: ContainerRuntimeError) -> Self {
        RepoError::new(error.to_string())
    }
}

/// Stable identifier for a registry credential row.
pub type RegistryCredentialId = Uuid;

/// Per-user container quota. The docker management service MUST
/// consult `check_container_quota` before every container create
/// and refuse any axis that would be exceeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerQuota {
    /// Owning user.
    pub user_id: Uuid,
    /// Maximum running containers at any one time.
    pub max_concurrent: u32,
    /// Maximum total containers the user may own.
    pub max_total: u32,
    /// Maximum per-container CPU percent (`0..=100`).
    pub cpu_pct_max: u8,
    /// Maximum per-container memory in bytes.
    pub memory_bytes_max: u64,
    /// Maximum monthly network egress in bytes.
    pub egress_bytes_per_month: u64,
    /// When the quota was last updated.
    pub updated_at: DateTime<Utc>,
}

impl ContainerQuota {
    /// Default quota as created for a new user.
    pub fn default_for(user_id: Uuid) -> Self {
        Self {
            user_id,
            max_concurrent: MAX_CONTAINERS_PER_USER,
            max_total: MAX_CONTAINERS_PER_USER * 2,
            cpu_pct_max: 50,
            memory_bytes_max: 1024 * 1024 * 1024,
            egress_bytes_per_month: MAX_EGRESS_BYTES_PER_MONTH,
            updated_at: Utc::now(),
        }
    }

    /// Validate the quota invariants.
    ///
    /// * Axes are monotonic (`>= 1`).
    /// * `cpu_pct_max` is in `0..=100`.
    /// * `max_concurrent <= max_total`.
    pub fn validate(&self) -> Result<(), ContainerRuntimeError> {
        if self.max_concurrent == 0 {
            return Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Concurrent,
            });
        }
        if self.max_total == 0 {
            return Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Total,
            });
        }
        if self.max_concurrent > self.max_total {
            return Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Total,
            });
        }
        if self.cpu_pct_max > 100 {
            return Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Cpu,
            });
        }
        if self.memory_bytes_max == 0 {
            return Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Memory,
            });
        }
        if self.egress_bytes_per_month == 0 {
            return Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Egress,
            });
        }
        Ok(())
    }
}

/// A container's live metrics snapshot at a single sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerMetrics {
    /// Owning user.
    pub user_id: Uuid,
    /// Container id (`Uuid` in our domain; the runtime-side id
    /// lives in the docker module).
    pub container_id: Uuid,
    /// CPU percent (`0..=100` * 100 for two-digit precision).
    pub cpu_pct: u16,
    /// Memory used (bytes).
    pub memory_bytes: u64,
    /// Network bytes received.
    pub net_rx: u64,
    /// Network bytes transmitted.
    pub net_tx: u64,
    /// Cumulative exit count since creation.
    pub exits: u32,
    /// When the sample was collected.
    pub sampled_at: DateTime<Utc>,
}

impl ContainerMetrics {
    /// Build a zeroed sample for `container_id` at `now`.
    pub fn empty(user_id: Uuid, container_id: Uuid, now: DateTime<Utc>) -> Self {
        Self {
            user_id,
            container_id,
            cpu_pct: 0,
            memory_bytes: 0,
            net_rx: 0,
            net_tx: 0,
            exits: 0,
            sampled_at: now,
        }
    }
}

/// A monthly network egress account: bytes sent by a user (or a
/// single container when `container_id` is `Some`) in `YYYY-MM`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkEgressAccount {
    /// Owning user.
    pub user_id: Uuid,
    /// Optional container scope; `None` means an aggregate row.
    pub container_id: Option<Uuid>,
    /// `YYYY-MM` month this row accounts for.
    pub month: String,
    /// Bytes egressed so far this month.
    pub bytes: u64,
}

impl NetworkEgressAccount {
    /// Build a fresh account for the supplied month.
    pub fn new(user_id: Uuid, container_id: Option<Uuid>, month: String) -> Self {
        Self {
            user_id,
            container_id,
            month,
            bytes: 0,
        }
    }

    /// Add `delta` bytes; saturates at `u64::MAX`.
    pub fn add(&mut self, delta: u64) {
        self.bytes = self.bytes.saturating_add(delta);
    }
}

/// Registry credential. `encrypted_secret` is the opaque
/// ciphertext produced by the app encryption helper
/// (`hex(nonce) || ":" || hex(ciphertext)` per the security
/// policy). The domain never decrypts; only the app layer does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryCredential {
    /// Stable credential id.
    pub id: RegistryCredentialId,
    /// Owning user.
    pub user_id: Uuid,
    /// Registry hostname (e.g. `registry.example.com`).
    pub registry: String,
    /// Username on that registry.
    pub username: String,
    /// Ciphertext of the secret, hex-encoded with the layout
    /// described in `Agents.md` §6.
    pub encrypted_secret: String,
    /// When the credential was created.
    pub created_at: DateTime<Utc>,
    /// When the credential was last used (pulled against).
    pub last_used_at: Option<DateTime<Utc>>,
}

impl RegistryCredential {
    /// Build a new credential row. The app layer is responsible
    /// for encrypting the secret before calling this.
    pub fn new(
        id: RegistryCredentialId,
        user_id: Uuid,
        registry: String,
        username: String,
        encrypted_secret: String,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            user_id,
            registry,
            username,
            encrypted_secret,
            created_at,
            last_used_at: None,
        }
    }

    /// Metadata-only projection: returns a copy of this
    /// credential with `encrypted_secret` blanked to the
    /// sentinel `"-"`. API/CLI responses MUST go through this
    /// helper so plaintext never escapes (see spec: Plaintext
    /// never echoed).
    pub fn redacted(&self) -> Self {
        Self {
            encrypted_secret: "-".into(),
            last_used_at: self.last_used_at,
            ..self.clone()
        }
    }
}

/// Result of applying a plan cap on top of the user-set quota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveQuota {
    /// The quota actually in force after the plan tightened any
    /// axes.
    pub quota: ContainerQuota,
    /// Per-axis indicators of whether the plan overrode the
    /// user-set value.
    pub plan_overrides: Vec<QuotaAxis>,
}

/// Apply the plan's per-axis caps on top of `user_quota`,
/// returning the effective quota and the list of axes the plan
/// tightened below the user-set value.
///
/// A `None` value in `plan` means "no plan cap on this axis";
/// the user-set value passes through unchanged.
pub fn effective_quota(user_quota: &ContainerQuota, plan: &PlanQuotaCaps) -> EffectiveQuota {
    let mut effective = user_quota.clone();
    let mut overrides = Vec::new();
    if let Some(cap) = plan.max_concurrent
        && cap < effective.max_concurrent
    {
        effective.max_concurrent = cap;
        overrides.push(QuotaAxis::Concurrent);
    }
    if let Some(cap) = plan.max_total
        && cap < effective.max_total
    {
        effective.max_total = cap;
        overrides.push(QuotaAxis::Total);
    }
    if let Some(cap) = plan.cpu_pct_max
        && cap < effective.cpu_pct_max
    {
        effective.cpu_pct_max = cap;
        overrides.push(QuotaAxis::Cpu);
    }
    if let Some(cap) = plan.memory_bytes_max
        && cap < effective.memory_bytes_max
    {
        effective.memory_bytes_max = cap;
        overrides.push(QuotaAxis::Memory);
    }
    if let Some(cap) = plan.egress_bytes_per_month
        && cap < effective.egress_bytes_per_month
    {
        effective.egress_bytes_per_month = cap;
        overrides.push(QuotaAxis::Egress);
    }
    EffectiveQuota {
        quota: effective,
        plan_overrides: overrides,
    }
}

/// Plan-imposed caps on container quota axes. A `None` value
/// means the plan does not cap that axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanQuotaCaps {
    /// Optional cap on `max_concurrent`.
    pub max_concurrent: Option<u32>,
    /// Optional cap on `max_total`.
    pub max_total: Option<u32>,
    /// Optional cap on `cpu_pct_max`.
    pub cpu_pct_max: Option<u8>,
    /// Optional cap on `memory_bytes_max`.
    pub memory_bytes_max: Option<u64>,
    /// Optional cap on `egress_bytes_per_month`.
    pub egress_bytes_per_month: Option<u64>,
}

/// Check whether `current_running + proposed` is within the
/// quota's `max_concurrent` axis.
///
/// Returns `Ok(())` when within the cap; otherwise
/// `Err(QuotaExceeded{axis: Concurrent})`.
pub fn check_concurrent(
    quota: &ContainerQuota,
    current_running: u32,
    proposed: u32,
) -> Result<(), ContainerRuntimeError> {
    if current_running.saturating_add(proposed) > quota.max_concurrent {
        return Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Concurrent,
        });
    }
    Ok(())
}

/// Check whether `current_total + proposed` is within the
/// quota's `max_total` axis.
pub fn check_total(
    quota: &ContainerQuota,
    current_total: u32,
    proposed: u32,
) -> Result<(), ContainerRuntimeError> {
    if current_total.saturating_add(proposed) > quota.max_total {
        return Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Total,
        });
    }
    Ok(())
}

/// Check whether `cpu_pct` is within the quota's `cpu_pct_max`
/// axis.
pub fn check_cpu(quota: &ContainerQuota, cpu_pct: u8) -> Result<(), ContainerRuntimeError> {
    if cpu_pct > quota.cpu_pct_max {
        return Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Cpu,
        });
    }
    Ok(())
}

/// Check whether `memory_bytes` is within the quota's
/// `memory_bytes_max` axis.
pub fn check_memory(
    quota: &ContainerQuota,
    memory_bytes: u64,
) -> Result<(), ContainerRuntimeError> {
    if memory_bytes > quota.memory_bytes_max {
        return Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Memory,
        });
    }
    Ok(())
}

/// Check whether `used + additional` leaves room for a new
/// transfer of `additional` bytes within the monthly egress
/// cap.
pub fn check_egress(
    quota: &ContainerQuota,
    used: u64,
    additional: u64,
) -> Result<(), ContainerRuntimeError> {
    if used.saturating_add(additional) > quota.egress_bytes_per_month {
        return Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Egress,
        });
    }
    Ok(())
}

/// Returns `true` when the supplied `month` (`YYYY-MM`) crosses
/// the 80% egress threshold (spec: Threshold cross scenario).
pub fn egress_threshold_crossed(quota: &ContainerQuota, used: u64) -> bool {
    let threshold = quota.egress_bytes_per_month / 100 * 80;
    used >= threshold
}

/// Persistence port for the container runtime bounded context.
#[async_trait]
pub trait ContainerRuntimeRepository: Send + Sync + 'static {
    /// Persist a per-user quota (insert-or-replace).
    async fn save_quota(&self, quota: &ContainerQuota) -> Result<(), RepoError>;
    /// Load a quota by owner id.
    async fn get_quota(&self, user_id: Uuid) -> Result<Option<ContainerQuota>, RepoError>;

    /// Append (insert) a metrics sample.
    async fn save_metrics(&self, metrics: &ContainerMetrics) -> Result<(), RepoError>;
    /// Return the most recent `limit` samples for `container_id`,
    /// oldest first.
    async fn list_metrics(
        &self,
        container_id: Uuid,
        limit: u32,
    ) -> Result<Vec<ContainerMetrics>, RepoError>;
    /// Trim metrics older than `before` for `container_id`.
    /// Returns the number of rows deleted.
    async fn trim_metrics(
        &self,
        container_id: Uuid,
        before: DateTime<Utc>,
    ) -> Result<u64, RepoError>;

    /// Persist (insert-or-replace) a monthly egress account.
    /// `bytes` overwrites the previous value.
    async fn save_egress(&self, account: &NetworkEgressAccount) -> Result<(), RepoError>;
    /// Load a monthly egress account.
    async fn get_egress(
        &self,
        user_id: Uuid,
        container_id: Option<Uuid>,
        month: &str,
    ) -> Result<Option<NetworkEgressAccount>, RepoError>;
    /// Increment the bytes for `(user_id, container_id, month)`
    /// and return the new total.
    async fn add_egress(
        &self,
        user_id: Uuid,
        container_id: Option<Uuid>,
        month: &str,
        delta: u64,
    ) -> Result<u64, RepoError>;

    /// Insert a registry credential.
    async fn save_credential(&self, credential: &RegistryCredential) -> Result<(), RepoError>;
    /// Load a credential by id.
    async fn get_credential_by_id(
        &self,
        id: RegistryCredentialId,
    ) -> Result<Option<RegistryCredential>, RepoError>;
    /// Load a credential by `(user_id, registry)`.
    async fn get_credential(
        &self,
        user_id: Uuid,
        registry: &str,
    ) -> Result<Option<RegistryCredential>, RepoError>;
    /// List credentials for a user (in insertion order).
    async fn list_credentials(&self, user_id: Uuid) -> Result<Vec<RegistryCredential>, RepoError>;
    /// Delete a credential by id; returns `true` when a row
    /// was removed.
    async fn delete_credential(&self, id: RegistryCredentialId) -> Result<bool, RepoError>;
    /// Mark `last_used_at` for a credential.
    async fn touch_credential(
        &self,
        id: RegistryCredentialId,
        at: DateTime<Utc>,
    ) -> Result<(), RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(user_id: Uuid) -> ContainerQuota {
        ContainerQuota::default_for(user_id)
    }

    #[test]
    fn default_quota_passes_validate() {
        let q = quota(Uuid::new_v4());
        q.validate().expect("default is valid");
    }

    #[test]
    fn validate_rejects_zero_concurrent() {
        let mut q = quota(Uuid::new_v4());
        q.max_concurrent = 0;
        assert!(matches!(
            q.validate(),
            Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Concurrent
            })
        ));
    }

    #[test]
    fn validate_rejects_concurrent_above_total() {
        let mut q = quota(Uuid::new_v4());
        q.max_total = 2;
        q.max_concurrent = 4;
        assert!(matches!(
            q.validate(),
            Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Total
            })
        ));
    }

    #[test]
    fn validate_rejects_cpu_above_100() {
        let mut q = quota(Uuid::new_v4());
        q.cpu_pct_max = 101;
        assert!(matches!(
            q.validate(),
            Err(ContainerRuntimeError::QuotaExceeded {
                axis: QuotaAxis::Cpu
            })
        ));
    }

    #[test]
    fn check_concurrent_rejects_overflow_and_accepts_within() {
        let q = quota(Uuid::new_v4());
        assert!(check_concurrent(&q, q.max_concurrent, 1).is_err());
        assert!(check_concurrent(&q, q.max_concurrent - 1, 1).is_ok());
    }

    #[test]
    fn check_total_rejects_overflow() {
        let q = quota(Uuid::new_v4());
        assert!(check_total(&q, q.max_total, 1).is_err());
        assert!(check_total(&q, q.max_total - 1, 1).is_ok());
    }

    #[test]
    fn check_cpu_rejects_overflow() {
        let q = quota(Uuid::new_v4());
        assert!(check_cpu(&q, q.cpu_pct_max + 1).is_err());
        assert!(check_cpu(&q, q.cpu_pct_max).is_ok());
    }

    #[test]
    fn check_memory_rejects_overflow() {
        let q = quota(Uuid::new_v4());
        assert!(check_memory(&q, q.memory_bytes_max + 1).is_err());
        assert!(check_memory(&q, q.memory_bytes_max).is_ok());
    }

    #[test]
    fn check_egress_rejects_overflow() {
        let q = quota(Uuid::new_v4());
        let max = q.egress_bytes_per_month;
        assert!(check_egress(&q, max.saturating_sub(100), 100).is_ok());
        assert!(check_egress(&q, max, 1).is_err());
    }

    #[test]
    fn egress_threshold_crossed_at_80pct() {
        let q = quota(Uuid::new_v4());
        let max = q.egress_bytes_per_month;
        let threshold = max / 100 * 80;
        assert!(!egress_threshold_crossed(&q, threshold - 1));
        assert!(egress_threshold_crossed(&q, threshold));
    }

    #[test]
    fn credential_redact_blanks_secret() {
        let cred = RegistryCredential::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "registry.example.com".into(),
            "u".into(),
            "deadbeef:cafe".into(),
            Utc::now(),
        );
        let r = cred.redacted();
        assert_eq!(r.encrypted_secret, "-");
        assert_eq!(r.username, cred.username);
        assert_eq!(r.registry, cred.registry);
    }

    #[test]
    fn effective_quota_tightens_on_plan_cap() {
        let user_q = quota(Uuid::new_v4());
        let plan = PlanQuotaCaps {
            max_concurrent: Some(2),
            ..Default::default()
        };
        let eff = effective_quota(&user_q, &plan);
        assert_eq!(eff.quota.max_concurrent, 2);
        assert_eq!(eff.plan_overrides, vec![QuotaAxis::Concurrent]);
    }

    #[test]
    fn effective_quota_passes_through_when_plan_looser() {
        let user_q = quota(Uuid::new_v4());
        // Plan allows more than the user-set value: no override.
        let plan = PlanQuotaCaps {
            max_concurrent: Some(user_q.max_concurrent + 10),
            ..Default::default()
        };
        let eff = effective_quota(&user_q, &plan);
        assert_eq!(eff.quota.max_concurrent, user_q.max_concurrent);
        assert!(eff.plan_overrides.is_empty());
    }

    #[test]
    fn egress_account_adds_saturate() {
        let mut acc = NetworkEgressAccount::new(Uuid::new_v4(), None, "2026-08".into());
        acc.add(100);
        assert_eq!(acc.bytes, 100);
        acc.add(u64::MAX);
        assert_eq!(acc.bytes, u64::MAX);
    }
}

#[cfg(test)]
mod prop {
    use super::*;

    proptest::proptest! {
        #[test]
        fn quota_validate_monotonic(max_concurrent in 1u32..=1024, max_total in 1u32..=1024) {
            let mut q = ContainerQuota::default_for(Uuid::new_v4());
            q.max_concurrent = max_concurrent;
            q.max_total = max_total.max(max_concurrent);
            q.cpu_pct_max = 50;
            q.memory_bytes_max = 1024;
            q.egress_bytes_per_month = 1024;
            proptest::prop_assert!(q.validate().is_ok());
        }

        #[test]
        fn egress_threshold_division_safe(monthly in 1u64..=u64::MAX / 2) {
            let mut q = ContainerQuota::default_for(Uuid::new_v4());
            q.egress_bytes_per_month = monthly;
            // Just assert it doesn't panic for any positive input.
            let _ = egress_threshold_crossed(&q, monthly);
        }
    }
}

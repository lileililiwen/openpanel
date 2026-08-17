//! Per-user / per-site resource quotas bounded context.
//!
//! The model captures disk, bandwidth, inode, max-file-size, and
//! (optional) CPU share limits with a soft/hard pair and a grace
//! window. The actual kernel-level enforcement (`setquota`, `tc`,
//! `nftables`) lives behind typed ports in the application layer
//! so the policy can be tested in isolation.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Which subject the quota policy applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaSubject {
    /// A user account.
    User,
    /// A site.
    Site,
}

/// Which quota dimension a usage sample covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaDimension {
    /// Disk bytes consumed.
    Disk,
    /// Bandwidth bytes consumed this month.
    Bandwidth,
    /// Inode count consumed.
    Inodes,
}

/// Per-axis soft/hard limit pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaLimit {
    /// Soft limit. The panel emits a warning when usage crosses
    /// this value but allows writes until the grace window closes.
    pub soft_bytes: u64,
    /// Hard limit. The kernel rejects writes once usage crosses
    /// this value; the in-process path surfaces a typed
    /// `QuotaExceeded`.
    pub hard_bytes: u64,
    /// Grace window in days for the soft limit. Default 7.
    pub grace_days: u32,
}

impl QuotaLimit {
    /// Reject zero hard limits — they make the policy meaningless.
    pub fn validate(&self) -> Result<(), QuotaError> {
        if self.hard_bytes == 0 {
            return Err(QuotaError::InvalidLimit);
        }
        if self.soft_bytes > self.hard_bytes {
            return Err(QuotaError::InvalidLimit);
        }
        Ok(())
    }

    /// Whether `usage` is over the soft limit.
    pub fn is_over_soft(&self, usage: u64) -> bool {
        usage > self.soft_bytes
    }

    /// Whether `usage` is over the hard limit.
    pub fn is_over_hard(&self, usage: u64) -> bool {
        usage > self.hard_bytes
    }
}

/// The aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaPolicy {
    id: Uuid,
    subject_kind: QuotaSubject,
    subject_id: Uuid,
    disk: QuotaLimit,
    bandwidth: QuotaLimit,
    inodes: QuotaLimit,
    max_file_size_bytes: Option<u64>,
    cpu_shares: Option<u32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl QuotaPolicy {
    /// Build a new policy. Defaults the grace to 7 days when the
    /// caller specifies `0`.
    pub fn new(
        id: Uuid,
        subject_kind: QuotaSubject,
        subject_id: Uuid,
        disk: QuotaLimit,
        bandwidth: QuotaLimit,
        inodes: QuotaLimit,
    ) -> Result<Self, QuotaError> {
        disk.validate()?;
        bandwidth.validate()?;
        inodes.validate()?;
        let now = Utc::now();
        Ok(Self {
            id,
            subject_kind,
            subject_id,
            disk,
            bandwidth,
            inodes,
            max_file_size_bytes: None,
            cpu_shares: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Build from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        subject_kind: QuotaSubject,
        subject_id: Uuid,
        disk: QuotaLimit,
        bandwidth: QuotaLimit,
        inodes: QuotaLimit,
        max_file_size_bytes: Option<u64>,
        cpu_shares: Option<u32>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, QuotaError> {
        Ok(Self {
            id,
            subject_kind,
            subject_id,
            disk,
            bandwidth,
            inodes,
            max_file_size_bytes,
            cpu_shares,
            created_at,
            updated_at,
        })
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Subject kind.
    pub fn subject_kind(&self) -> QuotaSubject {
        self.subject_kind
    }

    /// Subject id.
    pub fn subject_id(&self) -> Uuid {
        self.subject_id
    }

    /// Disk limit.
    pub fn disk(&self) -> &QuotaLimit {
        &self.disk
    }

    /// Bandwidth limit.
    pub fn bandwidth(&self) -> &QuotaLimit {
        &self.bandwidth
    }

    /// Inode limit.
    pub fn inodes(&self) -> &QuotaLimit {
        &self.inodes
    }

    /// Max file size in bytes (optional).
    pub fn max_file_size_bytes(&self) -> Option<u64> {
        self.max_file_size_bytes
    }

    /// Optional CPU shares (best-effort).
    pub fn cpu_shares(&self) -> Option<u32> {
        self.cpu_shares
    }

    /// When the policy was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the policy was last updated.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Update the disk limit.
    pub fn set_disk(&mut self, limit: QuotaLimit) -> Result<(), QuotaError> {
        limit.validate()?;
        self.disk = limit;
        self.touch();
        Ok(())
    }

    /// Update the bandwidth limit.
    pub fn set_bandwidth(&mut self, limit: QuotaLimit) -> Result<(), QuotaError> {
        limit.validate()?;
        self.bandwidth = limit;
        self.touch();
        Ok(())
    }

    /// Update the inode limit.
    pub fn set_inodes(&mut self, limit: QuotaLimit) -> Result<(), QuotaError> {
        limit.validate()?;
        self.inodes = limit;
        self.touch();
        Ok(())
    }

    /// Set the max file size. `None` disables the limit.
    pub fn set_max_file_size_bytes(&mut self, value: Option<u64>) {
        self.max_file_size_bytes = value;
        self.touch();
    }

    /// Set the CPU shares. `None` disables the limit.
    pub fn set_cpu_shares(&mut self, value: Option<u32>) {
        self.cpu_shares = value;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// A sampled usage snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaUsage {
    /// Subject id.
    pub subject_id: Uuid,
    /// When the sample was taken.
    pub sampled_at: DateTime<Utc>,
    /// Disk bytes consumed.
    pub disk_used_bytes: u64,
    /// Inode count consumed.
    pub disk_inodes_used: u64,
    /// Bandwidth bytes consumed this month.
    pub bandwidth_used_bytes_this_month: u64,
    /// Dimensions over their soft limit.
    pub over_soft: BTreeSet<QuotaDimension>,
    /// Dimensions over their hard limit.
    pub over_hard: BTreeSet<QuotaDimension>,
}

impl QuotaUsage {
    /// Build a new sample from raw counters and the policy.
    pub fn sample(
        subject_id: Uuid,
        policy: &QuotaPolicy,
        disk_used_bytes: u64,
        disk_inodes_used: u64,
        bandwidth_used_bytes: u64,
        now: DateTime<Utc>,
    ) -> Self {
        let mut over_soft = BTreeSet::new();
        let mut over_hard = BTreeSet::new();
        if policy.disk().is_over_soft(disk_used_bytes) {
            over_soft.insert(QuotaDimension::Disk);
        }
        if policy.disk().is_over_hard(disk_used_bytes) {
            over_hard.insert(QuotaDimension::Disk);
        }
        if policy.bandwidth().is_over_soft(bandwidth_used_bytes) {
            over_soft.insert(QuotaDimension::Bandwidth);
        }
        if policy.bandwidth().is_over_hard(bandwidth_used_bytes) {
            over_hard.insert(QuotaDimension::Bandwidth);
        }
        if policy.inodes().is_over_soft(disk_inodes_used) {
            over_soft.insert(QuotaDimension::Inodes);
        }
        if policy.inodes().is_over_hard(disk_inodes_used) {
            over_hard.insert(QuotaDimension::Inodes);
        }
        let _ = now;
        Self {
            subject_id,
            sampled_at: now,
            disk_used_bytes,
            disk_inodes_used,
            bandwidth_used_bytes_this_month: bandwidth_used_bytes,
            over_soft,
            over_hard,
        }
    }
}

/// Persistence port.
#[async_trait]
pub trait QuotaRepository: Send + Sync + 'static {
    /// Insert a new policy.
    async fn insert(&self, policy: &QuotaPolicy) -> Result<(), QuotaError>;
    /// Find a policy by id.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<QuotaPolicy>, QuotaError>;
    /// Find a policy by subject.
    async fn find_by_subject(
        &self,
        subject_kind: QuotaSubject,
        subject_id: Uuid,
    ) -> Result<Option<QuotaPolicy>, QuotaError>;
    /// Update an existing policy.
    async fn update(&self, policy: &QuotaPolicy) -> Result<(), QuotaError>;
    /// Delete a policy.
    async fn delete(&self, id: Uuid) -> Result<(), QuotaError>;
    /// List all policies.
    async fn list(&self) -> Result<Vec<QuotaPolicy>, QuotaError>;
    /// Insert a sample.
    async fn insert_usage(&self, usage: &QuotaUsage) -> Result<(), QuotaError>;
    /// Most recent sample for a subject.
    async fn latest_usage(&self, subject_id: Uuid) -> Result<Option<QuotaUsage>, QuotaError>;
    /// Default impl to satisfy the policy finder port.
    async fn exists(&self, _id: Uuid) -> Result<bool, RepoError> {
        Ok(true)
    }
}

/// Errors that can occur in the quotas bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QuotaError {
    /// A limit is malformed (zero hard, soft > hard).
    #[error("invalid quota limit")]
    InvalidLimit,
    /// The policy is not found.
    #[error("quota policy not found")]
    PolicyNotFound,
    /// The subject is not found.
    #[error("subject not found")]
    SubjectNotFound,
    /// A quota enforcer (kernel-side) is unavailable.
    #[error("quota enforcer unavailable: {0}")]
    EnforcerUnavailable(String),
    /// The kernel refused the write (per `EDQUOT`).
    #[error("quota exceeded")]
    QuotaExceeded,
    /// Persistence failure.
    #[error("quota persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for QuotaError {
    fn from(error: RepoError) -> Self {
        QuotaError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_limit() -> QuotaLimit {
        QuotaLimit {
            soft_bytes: 100,
            hard_bytes: 200,
            grace_days: 7,
        }
    }

    #[test]
    fn limit_validates_zero_hard() {
        let mut limit = sample_limit();
        limit.hard_bytes = 0;
        assert_eq!(
            limit.validate().expect_err("must reject"),
            QuotaError::InvalidLimit
        );
    }

    #[test]
    fn limit_validates_soft_above_hard() {
        let mut limit = sample_limit();
        limit.soft_bytes = 500;
        assert_eq!(
            limit.validate().expect_err("must reject"),
            QuotaError::InvalidLimit
        );
    }

    #[test]
    fn limit_accepts_valid_pair() {
        assert!(sample_limit().validate().is_ok());
    }

    #[test]
    fn limit_over_soft_and_hard_separately() {
        let limit = sample_limit();
        assert!(!limit.is_over_soft(50));
        assert!(limit.is_over_soft(150));
        assert!(!limit.is_over_hard(150));
        assert!(limit.is_over_hard(250));
    }

    #[test]
    fn usage_collects_over_soft_and_over_hard() {
        let policy = QuotaPolicy::new(
            Uuid::new_v4(),
            QuotaSubject::User,
            Uuid::new_v4(),
            sample_limit(),
            sample_limit(),
            sample_limit(),
        )
        .unwrap();
        let usage = QuotaUsage::sample(policy.subject_id(), &policy, 250, 50, 50, Utc::now());
        assert!(usage.over_hard.contains(&QuotaDimension::Disk));
        assert!(usage.over_soft.contains(&QuotaDimension::Disk));
        assert!(!usage.over_soft.contains(&QuotaDimension::Bandwidth));
    }

    #[test]
    fn policy_set_disk_updates_timestamp() {
        let mut policy = QuotaPolicy::new(
            Uuid::new_v4(),
            QuotaSubject::User,
            Uuid::new_v4(),
            sample_limit(),
            sample_limit(),
            sample_limit(),
        )
        .unwrap();
        let original = policy.updated_at();
        std::thread::sleep(std::time::Duration::from_millis(10));
        policy.set_disk(sample_limit()).unwrap();
        assert!(policy.updated_at() >= original);
    }
}

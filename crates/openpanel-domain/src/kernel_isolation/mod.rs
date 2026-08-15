//! Kernel resource isolation bounded context: per-user cgroup
//! limits and namespace configuration.
//!
//! The cgroup path is always
//! `/sys/fs/cgroup/openpanel/{user_id}`. Every limit is clamped
//! to a sane minimum (never zero-unbounded when the source value
//! was zero); every enforce failure is audited but never
//! disables isolation.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the kernel isolation bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IsolationError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The target user does not exist.
    #[error("user not found: {0}")]
    UserNotFound(Uuid),
    /// The requested policy is invalid.
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
    /// The cgroup write failed.
    #[error("cgroup write failed: {0}")]
    CgroupWrite(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<IsolationError> for RepoError {
    fn from(error: IsolationError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for IsolationError {
    fn from(error: RepoError) -> Self {
        IsolationError::Persistence(error.0)
    }
}

/// Per-user cgroup limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgroupLimit {
    /// Stable id (one row per user).
    pub user_id: Uuid,
    /// CPU quota in millicores (1000 == 1 core).
    pub cpu_millicores: u32,
    /// Memory high watermark in MiB.
    pub memory_high_mib: u32,
    /// Memory max in MiB.
    pub memory_max_mib: u32,
    /// Maximum number of processes.
    pub pids_max: u32,
    /// When the limit was last applied.
    pub updated_at: DateTime<Utc>,
}

impl CgroupLimit {
    /// Default limit for a new user.
    pub fn default_for(user_id: Uuid) -> Self {
        Self {
            user_id,
            cpu_millicores: 1000,
            memory_high_mib: 1024,
            memory_max_mib: 2048,
            pids_max: 256,
            updated_at: Utc::now(),
        }
    }

    /// Validate the limit is within a sensible range.
    pub fn validate(&self) -> Result<(), IsolationError> {
        if self.cpu_millicores == 0 {
            return Err(IsolationError::InvalidPolicy(
                "cpu_millicores must be > 0".into(),
            ));
        }
        if self.pids_max == 0 {
            return Err(IsolationError::InvalidPolicy("pids_max must be > 0".into()));
        }
        if self.memory_high_mib > self.memory_max_mib {
            return Err(IsolationError::InvalidPolicy(
                "memory_high_mib must be <= memory_max_mib".into(),
            ));
        }
        Ok(())
    }
}

/// Namespace configuration attached to every cgroup limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserNamespaceConfig {
    /// Owner of the namespace.
    pub user_id: Uuid,
    /// Whether the user namespace is enabled.
    pub enabled: bool,
    /// Whether the cgroup namespace is enabled.
    pub cgroup_namespace: bool,
    /// Whether the PID namespace is enabled.
    pub pid_namespace: bool,
}

impl UserNamespaceConfig {
    /// Default namespace config for a new user.
    pub fn default_for(user_id: Uuid) -> Self {
        Self {
            user_id,
            enabled: true,
            cgroup_namespace: true,
            pid_namespace: true,
        }
    }
}

/// Global isolation policy (defaults applied to new users).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationPolicy {
    /// When the policy was last updated.
    pub updated_at: DateTime<Utc>,
    /// Principal that last updated the policy.
    pub updated_by: Uuid,
    /// Default CPU limit (millicores).
    pub default_cpu_millicores: u32,
    /// Default memory high (MiB).
    pub default_memory_high_mib: u32,
    /// Default memory max (MiB).
    pub default_memory_max_mib: u32,
    /// Default PID cap.
    pub default_pids_max: u32,
}

impl Default for IsolationPolicy {
    fn default() -> Self {
        Self {
            updated_at: Utc::now(),
            updated_by: Uuid::nil(),
            default_cpu_millicores: 1000,
            default_memory_high_mib: 1024,
            default_memory_max_mib: 2048,
            default_pids_max: 256,
        }
    }
}

impl IsolationPolicy {
    /// Validate the policy values.
    pub fn validate(&self) -> Result<(), IsolationError> {
        if self.default_cpu_millicores == 0 {
            return Err(IsolationError::InvalidPolicy(
                "default_cpu_millicores must be > 0".into(),
            ));
        }
        if self.default_pids_max == 0 {
            return Err(IsolationError::InvalidPolicy(
                "default_pids_max must be > 0".into(),
            ));
        }
        if self.default_memory_high_mib > self.default_memory_max_mib {
            return Err(IsolationError::InvalidPolicy(
                "default_memory_high_mib must be <= default_memory_max_mib".into(),
            ));
        }
        Ok(())
    }

    /// Materialise a default per-user limit for a new user.
    pub fn default_limit_for(&self, user_id: Uuid) -> CgroupLimit {
        CgroupLimit {
            user_id,
            cpu_millicores: self.default_cpu_millicores,
            memory_high_mib: self.default_memory_high_mib,
            memory_max_mib: self.default_memory_max_mib,
            pids_max: self.default_pids_max,
            updated_at: self.updated_at,
        }
    }
}

/// Root cgroup path the panel owns.
pub const ROOT_CGROUP: &str = "/sys/fs/cgroup/openpanel";

/// Compose the per-user cgroup path.
pub fn user_cgroup_path(user_id: Uuid) -> String {
    format!("{ROOT_CGROUP}/{user_id}")
}

/// Persistence port for the kernel isolation bounded context.
#[async_trait]
pub trait IsolationRepository: Send + Sync + 'static {
    /// Persist a per-user cgroup limit.
    async fn save_limit(&self, limit: &CgroupLimit) -> Result<(), RepoError>;
    /// Load a limit by user id.
    async fn get_limit(&self, user_id: Uuid) -> Result<Option<CgroupLimit>, RepoError>;
    /// List all limits.
    async fn list_limits(&self) -> Result<Vec<CgroupLimit>, RepoError>;
    /// Delete a limit.
    async fn delete_limit(&self, user_id: Uuid) -> Result<(), RepoError>;

    /// Persist a namespace config.
    async fn save_namespace(&self, config: &UserNamespaceConfig) -> Result<(), RepoError>;
    /// Load a namespace config by user id.
    async fn get_namespace(
        &self,
        user_id: Uuid,
    ) -> Result<Option<UserNamespaceConfig>, RepoError>;

    /// Persist the singleton policy.
    async fn save_policy(&self, policy: &IsolationPolicy) -> Result<(), RepoError>;
    /// Load the singleton policy.
    async fn get_policy(&self) -> Result<Option<IsolationPolicy>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_cgroup_path_is_scoped() {
        let id = Uuid::new_v4();
        let path = user_cgroup_path(id);
        assert!(path.starts_with(ROOT_CGROUP));
        assert!(path.contains(&id.to_string()));
    }

    #[test]
    fn policy_validates_high_le_max() {
        let mut policy = IsolationPolicy::default();
        policy.default_memory_high_mib = 4096;
        policy.default_memory_max_mib = 1024;
        assert!(policy.validate().is_err());
    }

    #[test]
    fn limit_rejects_zero_cpu() {
        let mut limit = CgroupLimit::default_for(Uuid::new_v4());
        limit.cpu_millicores = 0;
        assert!(limit.validate().is_err());
    }

    #[test]
    fn limit_rejects_high_le_max() {
        let mut limit = CgroupLimit::default_for(Uuid::new_v4());
        limit.memory_high_mib = 100;
        limit.memory_max_mib = 50;
        assert!(limit.validate().is_err());
    }

    #[test]
    fn policy_default_limit_carries_values() {
        let mut policy = IsolationPolicy::default();
        policy.default_cpu_millicores = 2000;
        policy.default_pids_max = 128;
        let limit = policy.default_limit_for(Uuid::new_v4());
        assert_eq!(limit.cpu_millicores, 2000);
        assert_eq!(limit.pids_max, 128);
    }
}
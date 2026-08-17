//! Cron scope and per-user quota refinement.
//!
//! The `refine-cron-with-role-permissions` change adds the
//! `JobScope`, `CronQuota`, and role permission checker that
//! the scheduler consumes. The pre-existing `cron` module holds
//! the job aggregate, schedule, and persistence port.

use crate::identity::role::Role;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Scope of a cron job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobScope {
    /// System cron: working directory under `/etc/cron.*` or
    /// `/usr/local/sbin`; executable allow-list.
    System,
    /// Per-site cron: working directory must be under a site
    /// owned by the principal.
    PerSite,
    /// Per-user cron: arbitrary argv bounded by the per-user
    /// allow-list captured at user creation.
    PerUser,
}

/// Per-user quota dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronQuota {
    pub max_concurrent: u32,
    pub max_due_per_minute: u32,
    pub max_total: u32,
}

impl CronQuota {
    /// Default per-user quota.
    pub fn default_for_user() -> Self {
        Self {
            max_concurrent: 4,
            max_due_per_minute: 30,
            max_total: 100,
        }
    }
    /// Compute the effective quota as the per-axis max of the
    /// global default and the per-user override.
    pub fn effective(global: &GlobalDefault, user: &CronQuota) -> CronQuota {
        CronQuota {
            max_concurrent: global.max_concurrent.max(user.max_concurrent),
            max_due_per_minute: global.max_due_per_minute.max(user.max_due_per_minute),
            max_total: global.max_total.max(user.max_total),
        }
    }
}

/// Global default quota, used when no plan override is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalDefault {
    pub max_concurrent: u32,
    pub max_due_per_minute: u32,
    pub max_total: u32,
}

impl GlobalDefault {
    /// The default values used by the panel.
    pub const PANEL_DEFAULT: Self = Self {
        max_concurrent: 4,
        max_due_per_minute: 30,
        max_total: 100,
    };
}

/// Compile-time reference to the global default. Re-exported as
/// `cron_global_default` for backwards compatibility.
pub mod cron_global_default {
    pub use super::GlobalDefault;
    /// Convert to a runtime `CronQuota`.
    pub fn as_cron_quota(g: &super::GlobalDefault) -> super::CronQuota {
        super::CronQuota {
            max_concurrent: g.max_concurrent,
            max_due_per_minute: g.max_due_per_minute,
            max_total: g.max_total,
        }
    }
}

/// Errors that can occur in the cron scope / quota refinement.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CronScopeError {
    /// The principal may not create a job in the requested scope.
    #[error("role {role:?} may not create a {scope:?} job")]
    RoleNotAllowed { role: Role, scope: JobScope },
    /// Total quota exceeded.
    #[error("total quota exceeded")]
    QuotaExceededTotal,
    /// Concurrent quota exceeded.
    #[error("concurrent quota exceeded")]
    QuotaExceededConcurrent,
    /// Per-minute quota exceeded.
    #[error("per-minute quota exceeded")]
    QuotaExceededPerMinute,
    /// Working directory does not resolve under an owned site.
    #[error("working directory not under owned site")]
    WorkingDirectoryOutOfOwned,
    /// Executable is not in the principal's allow-list.
    #[error("executable not in allow-list")]
    ExecutableNotAllowed,
    /// Persistence failure.
    #[error("cron scope persistence error: {0}")]
    Persistence(String),
}

/// Role permission checker.
pub fn role_allows_scope(role: Role, scope: JobScope) -> bool {
    match (role, scope) {
        (Role::Owner, _) => true,
        (Role::Admin, JobScope::PerSite | JobScope::PerUser) => true,
        (Role::Admin, JobScope::System) => true,
        (Role::User, JobScope::PerSite | JobScope::PerUser) => true,
        (Role::User, JobScope::System) => false,
    }
}

/// Whether a working directory is under an owned site root.
pub fn is_under_owned_site(workdir: &std::path::Path, owned_roots: &[std::path::PathBuf]) -> bool {
    for root in owned_roots {
        if workdir.starts_with(root) {
            return true;
        }
    }
    false
}

/// Whether an executable path is in the per-user allow-list.
pub fn is_executable_allowed(executable: &str, allow_list: &[String]) -> bool {
    allow_list.iter().any(|allowed| allowed == executable)
}

/// Apply the quota check against the current state.
pub fn check_quota(
    quota: &CronQuota,
    total_active: u32,
    concurrent_active: u32,
    recently_due_in_minute: u32,
) -> Result<(), CronScopeError> {
    if total_active >= quota.max_total {
        return Err(CronScopeError::QuotaExceededTotal);
    }
    if concurrent_active >= quota.max_concurrent {
        return Err(CronScopeError::QuotaExceededConcurrent);
    }
    if recently_due_in_minute >= quota.max_due_per_minute {
        return Err(CronScopeError::QuotaExceededPerMinute);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_can_create_any_scope() {
        assert!(role_allows_scope(Role::Owner, JobScope::System));
        assert!(role_allows_scope(Role::Owner, JobScope::PerSite));
        assert!(role_allows_scope(Role::Owner, JobScope::PerUser));
    }

    #[test]
    fn admin_can_create_per_site_and_per_user() {
        assert!(role_allows_scope(Role::Admin, JobScope::PerSite));
        assert!(role_allows_scope(Role::Admin, JobScope::PerUser));
    }

    #[test]
    fn user_cannot_create_system_jobs() {
        assert!(!role_allows_scope(Role::User, JobScope::System));
        assert!(role_allows_scope(Role::User, JobScope::PerSite));
        assert!(role_allows_scope(Role::User, JobScope::PerUser));
    }

    #[test]
    fn workdir_must_be_under_owned_site() {
        let owned = vec![std::path::PathBuf::from("/var/www/site-a")];
        assert!(is_under_owned_site(
            std::path::Path::new("/var/www/site-a/public"),
            &owned
        ));
        assert!(!is_under_owned_site(
            std::path::Path::new("/var/www/site-b"),
            &owned
        ));
    }

    #[test]
    fn executable_allow_list_filter() {
        let allow = vec!["php".to_string(), "wp-cli".to_string()];
        assert!(is_executable_allowed("php", &allow));
        assert!(!is_executable_allowed("rm", &allow));
    }

    #[test]
    fn quota_check_rejects_total_overflow() {
        let quota = CronQuota::default_for_user();
        let err = check_quota(&quota, 100, 0, 0).expect_err("must reject");
        assert_eq!(err, CronScopeError::QuotaExceededTotal);
    }

    #[test]
    fn quota_check_rejects_concurrent_overflow() {
        let quota = CronQuota::default_for_user();
        let err = check_quota(&quota, 0, 4, 0).expect_err("must reject");
        assert_eq!(err, CronScopeError::QuotaExceededConcurrent);
    }

    #[test]
    fn quota_check_rejects_per_minute_overflow() {
        let quota = CronQuota::default_for_user();
        let err = check_quota(&quota, 0, 0, 30).expect_err("must reject");
        assert_eq!(err, CronScopeError::QuotaExceededPerMinute);
    }

    #[test]
    fn effective_quota_takes_per_axis_max() {
        let global = GlobalDefault::PANEL_DEFAULT;
        let user = CronQuota {
            max_concurrent: 8,
            max_due_per_minute: 100,
            max_total: 50,
        };
        let eff = CronQuota::effective(&global, &user);
        assert_eq!(eff.max_concurrent, 8);
        assert_eq!(eff.max_due_per_minute, 100);
        assert_eq!(eff.max_total, 100);
    }
}

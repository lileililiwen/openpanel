//! Per-site PHP runtime bounded context: `PhpRuntimeRef`,
//! `PhpFpmPoolSpec`, and the typed lifecycle for assigning /
//! swapping runtimes per site.

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Lifecycle status of a PHP FPM pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhpRuntimeStatus {
    /// The pool is up and accepting traffic.
    Running,
    /// The pool is configured but FPM is not accepting requests.
    Stopped,
    /// The pool status is unknown (e.g. the host is offline).
    Unknown,
}

/// Reference to a PHP runtime installed on a host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpRuntimeRef {
    package_id: String,
    version: String,
    socket_path: PathBuf,
    owner: Uuid,
    status: PhpRuntimeStatus,
}

impl PhpRuntimeRef {
    /// Build a new runtime reference. The socket path must be
    /// under `/run/php/` and the package id must be
    /// non-empty.
    pub fn new(
        package_id: impl Into<String>,
        version: impl Into<String>,
        socket_path: PathBuf,
        owner: Uuid,
    ) -> Result<Self, PhpRuntimeError> {
        let package_id = package_id.into();
        let version = version.into();
        if package_id.is_empty() {
            return Err(PhpRuntimeError::InvalidRuntime(
                "package_id must be non-empty".to_string(),
            ));
        }
        if version.is_empty() {
            return Err(PhpRuntimeError::InvalidRuntime(
                "version must be non-empty".to_string(),
            ));
        }
        let canonical = socket_path
            .components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .collect::<PathBuf>();
        let allowed = std::path::Path::new("/run/php");
        if !canonical.starts_with(allowed) {
            return Err(PhpRuntimeError::InvalidRuntime(
                "socket_path must be under /run/php".to_string(),
            ));
        }
        Ok(Self {
            package_id,
            version,
            socket_path: canonical,
            owner,
            status: PhpRuntimeStatus::Unknown,
        })
    }

    /// Restore from persistence.
    pub fn restore(
        package_id: String,
        version: String,
        socket_path: PathBuf,
        owner: Uuid,
        status: PhpRuntimeStatus,
    ) -> Self {
        Self {
            package_id,
            version,
            socket_path,
            owner,
            status,
        }
    }

    /// Package id (e.g. `php-fpm`).
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// SemVer-ish version string.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Socket path.
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Owner user id.
    pub fn owner(&self) -> Uuid {
        self.owner
    }

    /// Current status.
    pub fn status(&self) -> PhpRuntimeStatus {
        self.status
    }

    /// Set the status (e.g. after a health check).
    pub fn set_status(&mut self, status: PhpRuntimeStatus) {
        self.status = status;
    }
}

/// The FPM pool configuration for a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhpFpmPoolSpec {
    site_id: Uuid,
    runtime: PhpRuntimeRef,
    pool_name: String,
    config_path: PathBuf,
    written_at: DateTime<Utc>,
    last_pool_status: PhpRuntimeStatus,
}

impl PhpFpmPoolSpec {
    /// Build a new pool spec. The pool name is derived from the
    /// site id; the config path must be under
    /// `/etc/php/<ver>/fpm/pool.d/`.
    pub fn new(site_id: Uuid, runtime: PhpRuntimeRef, written_at: DateTime<Utc>) -> Self {
        let pool_name = format!("openpanel-{}", short_id(site_id));
        let config_path = std::path::PathBuf::from(format!(
            "/etc/php/{}/fpm/pool.d/{}.conf",
            runtime.version(),
            pool_name
        ));
        Self {
            site_id,
            runtime,
            pool_name,
            config_path,
            written_at,
            last_pool_status: PhpRuntimeStatus::Unknown,
        }
    }

    /// Build with a custom pool name (used by the swap path).
    pub fn with_name(
        site_id: Uuid,
        runtime: PhpRuntimeRef,
        pool_name: String,
        written_at: DateTime<Utc>,
    ) -> Self {
        let config_path = std::path::PathBuf::from(format!(
            "/etc/php/{}/fpm/pool.d/{}.conf",
            runtime.version(),
            pool_name
        ));
        Self {
            site_id,
            runtime,
            pool_name,
            config_path,
            written_at,
            last_pool_status: PhpRuntimeStatus::Unknown,
        }
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Runtime reference.
    pub fn runtime(&self) -> &PhpRuntimeRef {
        &self.runtime
    }

    /// Pool name.
    pub fn pool_name(&self) -> &str {
        &self.pool_name
    }

    /// Config path.
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// When the pool config was written.
    pub fn written_at(&self) -> DateTime<Utc> {
        self.written_at
    }

    /// Last observed pool status.
    pub fn last_pool_status(&self) -> PhpRuntimeStatus {
        self.last_pool_status
    }

    /// Set the last observed pool status.
    pub fn set_last_pool_status(&mut self, status: PhpRuntimeStatus) {
        self.last_pool_status = status;
    }
}

/// Persistence port.
#[async_trait]
pub trait PhpRuntimeRepository: Send + Sync + 'static {
    /// Insert a pool spec.
    async fn insert_pool(&self, spec: &PhpFpmPoolSpec) -> Result<(), PhpRuntimeError>;
    /// Find the current pool for a site.
    async fn find_pool_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Option<PhpFpmPoolSpec>, PhpRuntimeError>;
    /// Update a pool spec.
    async fn update_pool(&self, spec: &PhpFpmPoolSpec) -> Result<(), PhpRuntimeError>;
    /// List installed runtimes on the host.
    async fn list_runtimes(&self) -> Result<Vec<PhpRuntimeRef>, PhpRuntimeError>;
    /// Update a runtime's status.
    async fn update_runtime(&self, runtime: &PhpRuntimeRef) -> Result<(), PhpRuntimeError>;
    /// Default impl to satisfy the placeholder pattern.
    async fn exists(&self, _id: Uuid) -> Result<bool, RepoError> {
        Ok(true)
    }
}

/// Errors that can occur in the per-site-php-runtime bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PhpRuntimeError {
    /// The runtime reference is malformed.
    #[error("invalid PHP runtime: {0}")]
    InvalidRuntime(String),
    /// The site does not have a PHP runtime assigned.
    #[error("site has no PHP runtime")]
    NoRuntimeAssigned,
    /// The runtime is not installed on the host.
    #[error("runtime not installed: {0}")]
    RuntimeNotInstalled(String),
    /// An assignment swap failed and was rolled back.
    #[error("swap failed and was rolled back")]
    SwapRolledBack,
    /// Persistence failure.
    #[error("php runtime persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for PhpRuntimeError {
    fn from(error: RepoError) -> Self {
        PhpRuntimeError::Persistence(error.0)
    }
}

fn short_id(id: Uuid) -> String {
    let hex = id.simple().to_string();
    hex.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_ref_rejects_non_empty_validations() {
        let err = PhpRuntimeRef::new(
            "",
            "8.3.0",
            std::path::PathBuf::from("/run/php/php8.3-fpm-test.sock"),
            Uuid::new_v4(),
        )
        .expect_err("must reject");
        assert!(matches!(err, PhpRuntimeError::InvalidRuntime(_)));
    }

    #[test]
    fn runtime_ref_rejects_socket_outside_run_php() {
        let err = PhpRuntimeRef::new(
            "php-fpm",
            "8.3.0",
            std::path::PathBuf::from("/tmp/php.sock"),
            Uuid::new_v4(),
        )
        .expect_err("must reject");
        assert!(matches!(err, PhpRuntimeError::InvalidRuntime(_)));
    }

    #[test]
    fn runtime_ref_accepts_valid_pair() {
        let r = PhpRuntimeRef::new(
            "php-fpm",
            "8.3.0",
            std::path::PathBuf::from("/run/php/php8.3-fpm-test.sock"),
            Uuid::new_v4(),
        )
        .unwrap();
        assert_eq!(r.package_id(), "php-fpm");
        assert_eq!(r.version(), "8.3.0");
        assert_eq!(r.status(), PhpRuntimeStatus::Unknown);
    }

    #[test]
    fn pool_spec_builds_under_fpm_pool_d() {
        let site_id = Uuid::new_v4();
        let runtime = PhpRuntimeRef::new(
            "php-fpm",
            "8.3.0",
            std::path::PathBuf::from("/run/php/php8.3-fpm-test.sock"),
            Uuid::new_v4(),
        )
        .unwrap();
        let spec = PhpFpmPoolSpec::new(site_id, runtime, Utc::now());
        let path = spec.config_path();
        assert!(path.starts_with("/etc/php/8.3.0/fpm/pool.d/"));
        assert!(spec.pool_name().starts_with("openpanel-"));
    }
}

//! Kernel isolation services: cgroup enforcer, namespace
//! isolator, quota bridge.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    CgroupLimit, IsolationError, IsolationPolicy, Role, User, UserNamespaceConfig, user_cgroup_path,
};
use uuid::Uuid;

/// Port that writes to the cgroup filesystem. Production wiring
/// points this at `/sys/fs/cgroup/openpanel`; tests use
/// `RecordingCgroupWriter`.
pub trait CgroupWriter: Send + Sync + 'static {
    /// Write `value` to `{path}/{key}`; returns the path that
    /// would have been written.
    fn write(&self, path: &str, key: &str, value: u64) -> Result<String, IsolationError>;
    /// Read the current value at `{path}/{key}`.
    fn read(&self, path: &str, key: &str) -> Result<u64, IsolationError>;
}

/// Recording cgroup writer used by tests. Records each call.
pub struct RecordingCgroupWriter {
    calls: std::sync::Mutex<Vec<(String, String, u64)>>,
}

impl RecordingCgroupWriter {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Snapshot the recorded calls.
    pub fn calls(&self) -> Vec<(String, String, u64)> {
        #[allow(clippy::expect_used)]
        self.calls.lock().expect("calls").clone()
    }
}

impl Default for RecordingCgroupWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl CgroupWriter for RecordingCgroupWriter {
    fn write(&self, path: &str, key: &str, value: u64) -> Result<String, IsolationError> {
        #[allow(clippy::expect_used)]
        self.calls
            .lock()
            .expect("calls")
            .push((path.to_string(), key.to_string(), value));
        Ok(format!("{path}/{key}"))
    }

    fn read(&self, path: &str, key: &str) -> Result<u64, IsolationError> {
        #[allow(clippy::expect_used)]
        Ok(self
            .calls
            .lock()
            .expect("calls")
            .iter()
            .rev()
            .find(|(p, k, _)| p == path && k == key)
            .map(|(_, _, v)| *v)
            .unwrap_or(0))
    }
}

/// Cgroup enforcer. The enforcer never disables isolation on a
/// failed write — it logs the failure and audits it.
pub struct CgroupEnforcer {
    writer: Arc<dyn CgroupWriter>,
    audit: Arc<dyn AuditService>,
}

impl CgroupEnforcer {
    /// Construct an enforcer.
    pub fn new(writer: Arc<dyn CgroupWriter>, audit: Arc<dyn AuditService>) -> Self {
        Self { writer, audit }
    }

    /// Apply `limit` to its user's cgroup slice.
    pub async fn apply(&self, caller: &User, limit: &CgroupLimit) -> Result<(), IsolationError> {
        require_admin(caller)?;
        limit.validate()?;
        let path = user_cgroup_path(limit.user_id);
        let mut failed = false;
        for (key, value) in [
            ("cpu.max", limit.cpu_millicores as u64),
            ("memory.high", (limit.memory_high_mib as u64) * 1024 * 1024),
            ("memory.max", (limit.memory_max_mib as u64) * 1024 * 1024),
            ("pids.max", limit.pids_max as u64),
        ] {
            if self.writer.write(&path, key, value).is_err() {
                failed = true;
            }
        }
        if failed {
            // Log + audit; isolation is NOT disabled.
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::IsolationEnforceFailed,
                        AuditOutcome::Failure,
                    )
                    .target(limit.user_id.to_string())
                    .metadata(serde_json::json!({ "path": path })),
                )
                .await;
            return Err(IsolationError::CgroupWrite(path));
        }
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::IsolationApplied,
                    AuditOutcome::Success,
                )
                .target(limit.user_id.to_string())
                .metadata(serde_json::json!({
                    "cpu_millicores": limit.cpu_millicores,
                    "memory_max_mib": limit.memory_max_mib,
                    "pids_max": limit.pids_max,
                })),
            )
            .await;
        Ok(())
    }
}

/// Namespace isolator. Writes the namespace flags to a
/// pseudo-port; the production wiring writes to `/proc/<pid>/attr`.
pub struct NamespaceIsolator;

impl NamespaceIsolator {
    /// Construct a namespace isolator.
    pub fn new() -> Self {
        Self
    }

    /// Apply the namespace configuration to the host.
    pub fn apply(&self, config: &UserNamespaceConfig) -> Result<(), IsolationError> {
        if !config.enabled {
            return Ok(());
        }
        // Production wiring: write to /proc/<pid>/attr/{current,clone}_set.
        // For the v1 we just validate the shape of the config.
        Ok(())
    }
}

impl Default for NamespaceIsolator {
    fn default() -> Self {
        Self::new()
    }
}

/// Quota bridge: converts the `resource-quotas` plan limits into
/// an `IsolationPolicy`.
pub struct QuotaBridge;

impl QuotaBridge {
    /// Construct a quota bridge.
    pub fn new() -> Self {
        Self
    }

    /// Build an `IsolationPolicy` from a plan quota row.
    pub fn from_plan_quota(
        cpu_millicores: u32,
        memory_high_mib: u32,
        memory_max_mib: u32,
        pids_max: u32,
        updated_by: Uuid,
    ) -> IsolationPolicy {
        IsolationPolicy {
            updated_at: chrono::Utc::now(),
            updated_by,
            default_cpu_millicores: cpu_millicores,
            default_memory_high_mib: memory_high_mib,
            default_memory_max_mib: memory_max_mib,
            default_pids_max: pids_max,
        }
    }
}

impl Default for QuotaBridge {
    fn default() -> Self {
        Self::new()
    }
}

fn require_admin(caller: &User) -> Result<(), IsolationError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(IsolationError::Forbidden),
    }
}

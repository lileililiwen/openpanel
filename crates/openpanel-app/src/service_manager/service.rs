//! Service manager services: lister (read-only status), actor
//! (lifecycle), and a swappable `SystemCtl` port so tests can run
//! the full path without root.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, ServiceAction, ServiceActionRecord, ServiceError, ServiceInfo, ServiceManagerRepository,
    ServiceStatus, User, DEFAULT_ALLOWLIST, is_allowed,
};
use uuid::Uuid;

use crate::service_manager::SqliteServiceManagerRepository;

/// Port for executing `systemctl` actions. The default
/// implementation shells out; tests use `RecordingSystemCtl` to
/// verify the call surface without root.
#[async_trait::async_trait]
pub trait SystemCtl: Send + Sync + 'static {
    /// Query the unit's current state.
    async fn is_active(&self, name: &str) -> Result<ServiceStatus, ServiceError>;
    /// Query whether the unit is enabled at boot.
    async fn is_enabled(&self, name: &str) -> Result<bool, ServiceError>;
    /// Run a lifecycle action.
    async fn run(
        &self,
        name: &str,
        action: ServiceAction,
    ) -> Result<CommandOutput, ServiceError>;
    /// Fetch the last N journal lines for the unit.
    async fn recent_logs(&self, name: &str, limit: u32) -> Result<Vec<String>, ServiceError>;
}

/// Output of a `systemctl` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Exit code (0 = success).
    pub code: i32,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
}

impl CommandOutput {
    /// `true` when the underlying command exited successfully.
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

/// Real `SystemCtl` that shells out to `systemctl`. Used in
/// production; the binary path is fixed and arguments are escaped
/// from a closed allow-list.
pub struct RealSystemCtl {
    bin: String,
}

impl RealSystemCtl {
    /// Construct a real executor using `/bin/systemctl`.
    pub fn new() -> Self {
        Self {
            bin: "/bin/systemctl".to_string(),
        }
    }
}

impl Default for RealSystemCtl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SystemCtl for RealSystemCtl {
    async fn is_active(&self, name: &str) -> Result<ServiceStatus, ServiceError> {
        let output = run(&self.bin, &["is-active", name]).await?;
        Ok(ServiceStatus::from_systemd(output.stdout.trim()))
    }

    async fn is_enabled(&self, name: &str) -> Result<bool, ServiceError> {
        let output = run(&self.bin, &["is-enabled", name]).await?;
        Ok(output.stdout.trim() == "enabled")
    }

    async fn run(
        &self,
        name: &str,
        action: ServiceAction,
    ) -> Result<CommandOutput, ServiceError> {
        run(&self.bin, &[action.systemctl_verb(), name]).await
    }

    async fn recent_logs(&self, name: &str, limit: u32) -> Result<Vec<String>, ServiceError> {
        let output = run(
            self.bin.as_str(),
            &[
                "-n",
                &limit.to_string(),
                "-u",
                name,
                "--no-pager",
                "--output=cat",
            ],
        )
        .await?;
        Ok(output
            .stdout
            .lines()
            .map(|line| line.to_string())
            .collect())
    }
}

async fn run(bin: &str, args: &[&str]) -> Result<CommandOutput, ServiceError> {
    let output = std::process::Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| ServiceError::System(format!("failed to spawn {bin}: {e}")))?;
    Ok(CommandOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// In-memory `SystemCtl` for tests. Each call records its name and
/// action; the active/enabled status is configurable per unit.
pub struct RecordingSystemCtl {
    states: std::sync::Mutex<std::collections::HashMap<String, ServiceStatus>>,
    enabled: std::sync::Mutex<std::collections::HashMap<String, bool>>,
    calls: std::sync::Mutex<Vec<(String, ServiceAction)>>,
}

impl RecordingSystemCtl {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            states: std::sync::Mutex::new(std::collections::HashMap::new()),
            enabled: std::sync::Mutex::new(std::collections::HashMap::new()),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Set the unit's `Active` state.
    pub fn set_active(&self, name: &str, status: ServiceStatus) {
        self.states.lock().expect("states").insert(name.to_string(), status);
    }

    /// Set the unit's `enabled` flag.
    pub fn set_enabled(&self, name: &str, enabled: bool) {
        self.enabled.lock().expect("enabled").insert(name.to_string(), enabled);
    }

    /// Snapshot the calls that have been recorded.
    pub fn calls(&self) -> Vec<(String, ServiceAction)> {
        self.calls.lock().expect("calls").clone()
    }
}

impl Default for RecordingSystemCtl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SystemCtl for RecordingSystemCtl {
    async fn is_active(&self, name: &str) -> Result<ServiceStatus, ServiceError> {
        Ok(self
            .states
            .lock()
            .expect("states")
            .get(name)
            .copied()
            .unwrap_or(ServiceStatus::Unknown))
    }

    async fn is_enabled(&self, name: &str) -> Result<bool, ServiceError> {
        Ok(self
            .enabled
            .lock()
            .expect("enabled")
            .get(name)
            .copied()
            .unwrap_or(false))
    }

    async fn run(
        &self,
        name: &str,
        action: ServiceAction,
    ) -> Result<CommandOutput, ServiceError> {
        self.calls
            .lock()
            .expect("calls")
            .push((name.to_string(), action));
        let mut states = self.states.lock().expect("states");
        match action {
            ServiceAction::Start => {
                states.insert(name.to_string(), ServiceStatus::Active);
            }
            ServiceAction::Stop => {
                states.insert(name.to_string(), ServiceStatus::Inactive);
            }
            ServiceAction::Restart => {
                states.insert(name.to_string(), ServiceStatus::Active);
            }
            ServiceAction::Enable => {
                self.enabled
                    .lock()
                    .expect("enabled")
                    .insert(name.to_string(), true);
            }
            ServiceAction::Disable => {
                self.enabled
                    .lock()
                    .expect("enabled")
                    .insert(name.to_string(), false);
            }
        }
        Ok(CommandOutput {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    async fn recent_logs(&self, name: &str, _limit: u32) -> Result<Vec<String>, ServiceError> {
        Ok(vec![format!("recorder: {name} sample line")])
    }
}

/// Read-only lister. The lister MUST NOT mutate state; it returns
/// a snapshot for each allow-listed unit.
pub struct ServiceLister {
    systemctl: Arc<dyn SystemCtl>,
    allowlist: Vec<String>,
    log_lines: u32,
}

impl ServiceLister {
    /// Construct a lister with the default allow-list.
    pub fn new(systemctl: Arc<dyn SystemCtl>) -> Self {
        Self::with_allowlist(systemctl, DEFAULT_ALLOWLIST.iter().map(|s| s.to_string()).collect())
    }

    /// Construct a lister with a custom allow-list.
    pub fn with_allowlist(systemctl: Arc<dyn SystemCtl>, allowlist: Vec<String>) -> Self {
        Self {
            systemctl,
            allowlist,
            log_lines: 20,
        }
    }

    /// List the allow-listed services with status, enabled flag,
    /// and recent log lines.
    pub async fn list(&self) -> Result<Vec<ServiceInfo>, ServiceError> {
        let mut out = Vec::new();
        for name in &self.allowlist {
            let status = self.systemctl.is_active(name).await?;
            let enabled = self.systemctl.is_enabled(name).await?;
            let recent_logs = self.systemctl.recent_logs(name, self.log_lines).await?;
            out.push(ServiceInfo {
                name: name.clone(),
                description: format!("systemd unit {name}"),
                status,
                enabled,
                recent_logs,
            });
        }
        Ok(out)
    }
}

/// Lifecycle actor. Every action is Admin-gated, allow-list
/// checked, executed through the swappable `SystemCtl` port, and
/// audited.
pub struct ServiceActor {
    repo: Arc<SqliteServiceManagerRepository>,
    audit: Arc<dyn AuditService>,
    systemctl: Arc<dyn SystemCtl>,
}

impl ServiceActor {
    /// Construct an actor with the default port.
    pub fn new(
        repo: Arc<SqliteServiceManagerRepository>,
        audit: Arc<dyn AuditService>,
        systemctl: Arc<dyn SystemCtl>,
    ) -> Self {
        Self {
            repo,
            audit,
            systemctl,
        }
    }

    /// Run a lifecycle action. The `name` is checked against the
    /// default allow-list before any command runs; unknown units
    /// are refused with `ServiceError::NotAllowed`.
    pub async fn act(
        &self,
        caller: &User,
        name: &str,
        action: ServiceAction,
    ) -> Result<ServiceActionRecord, ServiceError> {
        require_admin(caller)?;
        if !is_allowed(name) {
            return Err(ServiceError::NotAllowed(name.to_string()));
        }
        let output = self.systemctl.run(name, action).await?;
        let success = output.success();
        let message = if success {
            output.stdout.clone()
        } else {
            output.stderr.clone()
        };
        let record = ServiceActionRecord {
            id: Uuid::new_v4(),
            name: name.to_string(),
            action,
            actor: caller.id(),
            recorded_at: chrono::Utc::now(),
            success,
            message,
        };
        self.repo.save_action(&record).await?;
        let outcome = if success {
            AuditOutcome::Success
        } else {
            AuditOutcome::Failure
        };
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::ServiceChanged,
                    outcome,
                )
                .target(name.to_string())
                .metadata(serde_json::json!({
                    "action": action.as_str(),
                    "record_id": record.id.to_string(),
                })),
            )
            .await;
        Ok(record)
    }
}

fn require_admin(caller: &User) -> Result<(), ServiceError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(ServiceError::Forbidden),
    }
}

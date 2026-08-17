//! OS update services: lister, applier, and unattended config.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    OsUpdateError, OsUpdateRepository, PackageUpdate, RebootState, Role, UpdateHistoryRecord,
    UpdateKind, UpdatePolicy, User,
};
use uuid::Uuid;

use crate::os_updates::SqliteOsUpdateRepository;

/// Output of a package-manager command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Exit code.
    pub code: i32,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
}

/// Port that talks to the host package manager. Production wiring
/// runs `apt-get`; tests use `RecordingPackageManager`.
#[async_trait::async_trait]
pub trait PackageManager: Send + Sync + 'static {
    /// Run `apt-get -s upgrade` and return its stdout.
    async fn simulate_upgrade(&self) -> Result<CommandOutput, OsUpdateError>;
    /// Run `apt-get -y upgrade` (or `dist-upgrade` based on kind) and
    /// return its output.
    async fn apply_upgrade(&self, kind: UpdateKind) -> Result<CommandOutput, OsUpdateError>;
    /// Inspect the running kernel package and report whether the
    /// last apply touched it.
    async fn kernel_updated(&self) -> Result<bool, OsUpdateError>;
}

/// Default `PackageManager` that shells out to `apt-get`.
pub struct AptPackageManager {
    bin: String,
}

impl AptPackageManager {
    /// Construct an `apt-get`-backed manager.
    pub fn new() -> Self {
        Self {
            bin: "/usr/bin/apt-get".to_string(),
        }
    }
}

impl Default for AptPackageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PackageManager for AptPackageManager {
    async fn simulate_upgrade(&self) -> Result<CommandOutput, OsUpdateError> {
        run(&self.bin, &["-s", "upgrade"]).await
    }

    async fn apply_upgrade(&self, kind: UpdateKind) -> Result<CommandOutput, OsUpdateError> {
        let verb = match kind {
            UpdateKind::Security => "upgrade",
            UpdateKind::Other => "upgrade",
        };
        run(&self.bin, &["-y", verb]).await
    }

    async fn kernel_updated(&self) -> Result<bool, OsUpdateError> {
        let output = run(self.bin.as_str(), &["-s", "upgrade"]).await?;
        Ok(crate::os_updates::parse_apt_dry_run(&output.stdout)
            .iter()
            .any(|u| u.name.starts_with("linux-image") || u.name.starts_with("linux-headers")))
    }
}

async fn run(bin: &str, args: &[&str]) -> Result<CommandOutput, OsUpdateError> {
    let output = std::process::Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| OsUpdateError::Apt(format!("failed to spawn {bin}: {e}")))?;
    Ok(CommandOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Recording package manager used by tests.
pub struct RecordingPackageManager {
    simulate: std::sync::Mutex<String>,
    apply_calls: std::sync::Mutex<Vec<UpdateKind>>,
    kernel_updated: std::sync::Mutex<bool>,
}

impl RecordingPackageManager {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            simulate: std::sync::Mutex::new(String::new()),
            apply_calls: std::sync::Mutex::new(Vec::new()),
            kernel_updated: std::sync::Mutex::new(false),
        }
    }

    /// Set the simulated upgrade output.
    pub fn set_simulate_output(&self, output: impl Into<String>) {
        #[allow(clippy::expect_used)] // poisoned test-double mutex is a programming error
        let mut guard = self.simulate.lock().expect("simulate");
        *guard = output.into();
    }

    /// Mark whether the kernel was updated.
    pub fn set_kernel_updated(&self, value: bool) {
        #[allow(clippy::expect_used)] // poisoned test-double mutex is a programming error
        let mut guard = self.kernel_updated.lock().expect("kernel");
        *guard = value;
    }

    /// Snapshot apply calls.
    pub fn apply_calls(&self) -> Vec<UpdateKind> {
        #[allow(clippy::expect_used)] // poisoned test-double mutex is a programming error
        self.apply_calls.lock().expect("apply").clone()
    }
}

impl Default for RecordingPackageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PackageManager for RecordingPackageManager {
    async fn simulate_upgrade(&self) -> Result<CommandOutput, OsUpdateError> {
        #[allow(clippy::expect_used)] // poisoned test-double mutex is a programming error
        let stdout = self.simulate.lock().expect("simulate").clone();
        Ok(CommandOutput {
            code: 0,
            stdout,
            stderr: String::new(),
        })
    }

    async fn apply_upgrade(&self, kind: UpdateKind) -> Result<CommandOutput, OsUpdateError> {
        #[allow(clippy::expect_used)] // poisoned test-double mutex is a programming error
        self.apply_calls.lock().expect("apply").push(kind);
        Ok(CommandOutput {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    async fn kernel_updated(&self) -> Result<bool, OsUpdateError> {
        #[allow(clippy::expect_used)] // poisoned test-double mutex is a programming error
        let updated = *self.kernel_updated.lock().expect("kernel");
        Ok(updated)
    }
}

/// Read-only lister. `list()` returns the simulated-upgrade parse.
pub struct OsUpdateLister {
    package_manager: Arc<dyn PackageManager>,
}

impl OsUpdateLister {
    /// Construct a lister.
    pub fn new(package_manager: Arc<dyn PackageManager>) -> Self {
        Self { package_manager }
    }

    /// Run the simulate and parse the output.
    pub async fn list(&self) -> Result<Vec<PackageUpdate>, OsUpdateError> {
        let output = self.package_manager.simulate_upgrade().await?;
        Ok(crate::os_updates::parse_apt_dry_run(&output.stdout))
    }
}

/// Applier: runs a single apply (security or other), records
/// history, and audits the result. Admin/Owner only.
pub struct OsUpdateApplier {
    repo: Arc<SqliteOsUpdateRepository>,
    audit: Arc<dyn AuditService>,
    package_manager: Arc<dyn PackageManager>,
}

impl OsUpdateApplier {
    /// Construct an applier.
    pub fn new(
        repo: Arc<SqliteOsUpdateRepository>,
        audit: Arc<dyn AuditService>,
        package_manager: Arc<dyn PackageManager>,
    ) -> Self {
        Self {
            repo,
            audit,
            package_manager,
        }
    }

    /// Apply updates of the given kind.
    pub async fn apply(
        &self,
        caller: &User,
        kind: UpdateKind,
    ) -> Result<UpdateHistoryRecord, OsUpdateError> {
        require_admin(caller)?;
        let started_at = chrono::Utc::now();
        let output = self.package_manager.apply_upgrade(kind).await?;
        let success = output.code == 0;
        let kernel_updated = self.package_manager.kernel_updated().await?;
        let reboot = RebootState {
            required: kernel_updated,
            kernel_updated,
        };
        let record = UpdateHistoryRecord {
            id: Uuid::new_v4(),
            actor: caller.id(),
            started_at,
            completed_at: Some(chrono::Utc::now()),
            kind,
            package_count: 0,
            success,
            message: if success {
                output.stdout.clone()
            } else {
                output.stderr.clone()
            },
            reboot,
        };
        self.repo.save_history(&record).await?;
        let outcome = if success {
            AuditOutcome::Success
        } else {
            AuditOutcome::Failure
        };
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::OsUpdateApplied,
                    outcome,
                )
                .target(kind.as_str().to_string())
                .metadata(serde_json::json!({
                    "record_id": record.id.to_string(),
                    "reboot_required": reboot.required,
                })),
            )
            .await;
        Ok(record)
    }
}

/// Writes the unattended-upgrades config to a path on disk.
pub struct UnattendedConfig {
    path: std::path::PathBuf,
}

impl UnattendedConfig {
    /// Construct a writer pointing at the given path.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Default path: `/etc/apt/apt.conf.d/20auto-upgrades`.
    pub fn default_path() -> std::path::PathBuf {
        std::path::PathBuf::from("/etc/apt/apt.conf.d/20auto-upgrades")
    }

    /// Render and write the policy. Returns the bytes written.
    pub fn write(&self, policy: &UpdatePolicy) -> Result<usize, OsUpdateError> {
        let body = policy.render_apt_config();
        std::fs::write(&self.path, &body)
            .map_err(|e| OsUpdateError::Apt(format!("failed to write config: {e}")))?;
        Ok(body.len())
    }

    /// Path the writer targets.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

fn require_admin(caller: &User) -> Result<(), OsUpdateError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(OsUpdateError::Forbidden),
    }
}

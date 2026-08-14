//! OS update management bounded context: package updates, unattended
//! upgrades policy, and reboot state.
//!
//! The applier is allow-listed: every command is composed from a
//! fixed verb (`upgrade` / `update`) and the closed set of
//! arguments shipped with this crate. No free-form shell string
//! ever reaches the executor.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the OS updates bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OsUpdateError {
    /// The caller is not authorised (Admin role required).
    #[error("forbidden")]
    Forbidden,
    /// The provided policy is invalid.
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
    /// The package manager command failed.
    #[error("apt failed: {0}")]
    Apt(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<OsUpdateError> for RepoError {
    fn from(error: OsUpdateError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for OsUpdateError {
    fn from(error: RepoError) -> Self {
        OsUpdateError::Persistence(error.0)
    }
}

/// Classification of an update by origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateKind {
    /// Security update (`-security` pocket).
    Security,
    /// Non-security update.
    Other,
}

impl UpdateKind {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Security => "security",
            Self::Other => "other",
        }
    }
}

/// A single package update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageUpdate {
    /// Package name.
    pub name: String,
    /// Currently installed version.
    pub current_version: String,
    /// Candidate version.
    pub candidate_version: String,
    /// Security vs other origin.
    pub kind: UpdateKind,
    /// Short summary (one-line).
    #[serde(default)]
    pub summary: String,
}

impl PackageUpdate {
    /// Construct a `PackageUpdate`.
    pub fn new(
        name: impl Into<String>,
        current: impl Into<String>,
        candidate: impl Into<String>,
        kind: UpdateKind,
    ) -> Self {
        Self {
            name: name.into(),
            current_version: current.into(),
            candidate_version: candidate.into(),
            kind,
            summary: String::new(),
        }
    }
}

/// Reboot state after the last apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RebootState {
    /// Whether a reboot is required.
    pub required: bool,
    /// Kernel package updated.
    pub kernel_updated: bool,
}

/// Unattended-upgrades policy persisted to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePolicy {
    /// Whether security updates are auto-installed.
    pub security_auto_install: bool,
    /// Whether non-security updates are auto-installed.
    pub other_auto_install: bool,
    /// Hour of day the unattended run fires (0..=23).
    pub run_hour: u8,
    /// Whether to auto-reboot when a kernel update lands.
    pub auto_reboot: bool,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self {
            security_auto_install: true,
            other_auto_install: false,
            run_hour: 4,
            auto_reboot: false,
        }
    }
}

impl UpdatePolicy {
    /// Validate the policy is in a sensible range.
    pub fn validate(&self) -> Result<(), OsUpdateError> {
        if self.run_hour > 23 {
            return Err(OsUpdateError::InvalidPolicy(
                "run_hour must be in 0..=23".into(),
            ));
        }
        Ok(())
    }

    /// Render the policy as the body of
    /// `/etc/apt/apt.conf.d/20auto-upgrades`. The file is regenerated
    /// from scratch on every set so the contents always reflect the
    /// current policy.
    pub fn render_apt_config(&self) -> String {
        let sec = if self.security_auto_install { "1" } else { "0" };
        let reboot = if self.auto_reboot { "true" } else { "false" };
        format!(
            r#"APT::Periodic::Update-Package-Lists "{sec}";
APT::Periodic::Unattended-Upgrade "{sec}";
APT::Periodic::AutocleanInterval "7";
APT::Periodic::Download-Upgradeable-Files "1";
Unattended-Upgrade::Allowed-Origins {{ "Debian stable"; "Debian security"; }};
Unattended-Upgrade::DevRelease "false";
Unattended-Upgrade::AutoFixInterruptedDpkg "true";
Unattended-Upgrade::MinimalSteps "true";
Unattended-Upgrade::InstallOnShutdown "false";
Unattended-Upgrade::Remove-Unused-Kernel-Packages "true";
Unattended-Upgrade::Remove-New-Unused-Dependencies "true";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
Unattended-Upgrade::Automatic-Reboot "{reboot}";
Unattended-Upgrade::Automatic-Reboot-Time "0{run_hour}:00";
"#,
            sec = sec,
            reboot = reboot,
            run_hour = self.run_hour,
        )
    }
}

/// One row in the apply history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateHistoryRecord {
    /// Stable id.
    pub id: Uuid,
    /// Principal that initiated the apply.
    pub actor: Uuid,
    /// When the apply started.
    pub started_at: DateTime<Utc>,
    /// When the apply completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// What was applied.
    pub kind: UpdateKind,
    /// Total packages updated.
    pub package_count: u32,
    /// Whether the apply succeeded.
    pub success: bool,
    /// Optional error or summary.
    #[serde(default)]
    pub message: String,
    /// Reboot state observed at completion.
    pub reboot: RebootState,
}

/// Persistence port for the OS updates bounded context.
#[async_trait]
pub trait OsUpdateRepository: Send + Sync + 'static {
    /// Persist the singleton policy.
    async fn save_policy(&self, policy: &UpdatePolicy) -> Result<(), RepoError>;
    /// Load the policy (singleton).
    async fn get_policy(&self) -> Result<Option<UpdatePolicy>, RepoError>;

    /// Persist an apply history row.
    async fn save_history(&self, record: &UpdateHistoryRecord) -> Result<(), RepoError>;
    /// List recent history rows, newest first.
    async fn list_history(&self, limit: u32) -> Result<Vec<UpdateHistoryRecord>, RepoError>;
}

//! Service manager bounded context: `ServiceInfo`, `ServiceAction`,
//! and `ServiceStatus`. Service names are validated against a
//! fixed allow-list before any system command runs; there is no
//! path through which an arbitrary unit name reaches `systemctl`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the service manager bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    /// The caller is not authorised (Admin role required).
    #[error("forbidden")]
    Forbidden,
    /// The unit name is not in the allow-list.
    #[error("service not allowed: {0}")]
    NotAllowed(String),
    /// The action verb is not in the allow-list.
    #[error("unknown action: {0}")]
    UnknownAction(String),
    /// The system command failed.
    #[error("systemctl failed: {0}")]
    System(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<ServiceError> for RepoError {
    fn from(error: ServiceError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for ServiceError {
    fn from(error: RepoError) -> Self {
        ServiceError::Persistence(error.0)
    }
}

/// Lifecycle state of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    /// The unit is running.
    Active,
    /// The unit is loaded but not running.
    Inactive,
    /// The unit failed during start or while running.
    Failed,
    /// The unit is in an unknown / transitional state.
    Unknown,
}

impl ServiceStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }

    /// Map a systemd `ActiveState` string to a `ServiceStatus`.
    pub fn from_systemd(label: &str) -> Self {
        match label {
            "active" => Self::Active,
            "inactive" | "deactivating" => Self::Inactive,
            "failed" => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

/// Lifecycle action verb. Only the five allow-listed verbs can be
/// executed; everything else is rejected before any command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    /// `systemctl start <name>`.
    Start,
    /// `systemctl stop <name>`.
    Stop,
    /// `systemctl restart <name>`.
    Restart,
    /// `systemctl enable <name>`.
    Enable,
    /// `systemctl disable <name>`.
    Disable,
}

impl ServiceAction {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Enable => "enable",
            Self::Disable => "disable",
        }
    }

    /// Map a `systemctl` verb to a `ServiceAction`. Returns `None`
    /// for any verb that is not on the allow-list.
    pub fn from_verb(verb: &str) -> Option<Self> {
        match verb {
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "restart" => Some(Self::Restart),
            "enable" => Some(Self::Enable),
            "disable" => Some(Self::Disable),
            _ => None,
        }
    }

    /// The exact `systemctl` subcommand the action maps to.
    pub fn systemctl_verb(&self) -> &'static str {
        self.as_str()
    }
}

/// Service description surfaced to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Unit name (must be on the allow-list).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Current lifecycle state.
    pub status: ServiceStatus,
    /// Whether the unit is enabled at boot.
    pub enabled: bool,
    /// Most recent journal lines.
    #[serde(default)]
    pub recent_logs: Vec<String>,
}

impl ServiceInfo {
    /// Construct a `ServiceInfo` with empty logs.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            status: ServiceStatus::Unknown,
            enabled: false,
            recent_logs: Vec::new(),
        }
    }
}

/// Recorded service action history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceActionRecord {
    /// Stable id.
    pub id: Uuid,
    /// Affected unit.
    pub name: String,
    /// Verb that was executed.
    pub action: ServiceAction,
    /// Principal that initiated the action.
    pub actor: Uuid,
    /// When the action was recorded.
    pub recorded_at: DateTime<Utc>,
    /// Whether the action succeeded.
    pub success: bool,
    /// Optional error / status output.
    #[serde(default)]
    pub message: String,
}

/// Persistence port for the service manager bounded context.
#[async_trait]
pub trait ServiceManagerRepository: Send + Sync + 'static {
    /// Persist a service action record.
    async fn save_action(&self, record: &ServiceActionRecord) -> Result<(), RepoError>;
    /// List recent action records.
    async fn list_actions(&self, limit: u32) -> Result<Vec<ServiceActionRecord>, RepoError>;
    /// List action records for one unit.
    async fn list_actions_for(
        &self,
        name: &str,
        limit: u32,
    ) -> Result<Vec<ServiceActionRecord>, RepoError>;
}

/// Default allow-list of unit names the service manager exposes.
/// Adding a unit here is the ONLY way to expose it to the UI; the
/// executor refuses every other name before any command runs.
pub const DEFAULT_ALLOWLIST: &[&str] = &[
    "nginx",
    "php8.1-fpm",
    "php8.2-fpm",
    "php8.3-fpm",
    "php8.4-fpm",
    "mysql",
    "mariadb",
    "redis",
    "redis-server",
    "postgresql",
    "ssh",
    "sshd",
    "cron",
    "rsyslog",
    "fail2ban",
    "ufw",
    "docker",
    "containerd",
    "openpanel",
    "openpanel-agent",
];

/// Return `true` when `name` is in the default allow-list.
pub fn is_allowed(name: &str) -> bool {
    DEFAULT_ALLOWLIST.iter().any(|unit| *unit == name)
}

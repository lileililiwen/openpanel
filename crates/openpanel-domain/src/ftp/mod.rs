//! Per-site FTP account domain model and persistence port.

use std::{
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Password, RepoError};

const DEFAULT_BANDWIDTH_KB: u64 = 1_048_576;
const DEFAULT_MAX_CONNECTIONS: u16 = 4;

/// FTP domain validation, authorization, or persistence error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FtpError {
    /// The caller cannot access the requested site.
    #[error("forbidden")]
    Forbidden,
    /// The requested site does not exist.
    #[error("site not found")]
    SiteNotFound,
    /// The requested account does not exist.
    #[error("FTP account not found")]
    NotFound,
    /// The username is already used within the site.
    #[error("FTP username already exists for this site")]
    Duplicate,
    /// Input violates an FTP invariant.
    #[error("invalid FTP value: {0}")]
    Invalid(String),
    /// Authentication was denied without exposing its cause.
    #[error("login incorrect")]
    LoginDenied,
    /// A connection ceiling was reached.
    #[error("FTP connection limit reached")]
    ConnectionLimit,
    /// A requested file operation is not permitted.
    #[error("FTP operation denied")]
    OperationDenied,
    /// Persistence or adapter failure.
    #[error("FTP operation failed: {0}")]
    Internal(String),
}

/// Per-account transfer and concurrency ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FtpLimits {
    bandwidth_kb_per_session: u64,
    max_concurrent_connections: u16,
}

impl Default for FtpLimits {
    fn default() -> Self {
        Self {
            bandwidth_kb_per_session: DEFAULT_BANDWIDTH_KB,
            max_concurrent_connections: DEFAULT_MAX_CONNECTIONS,
        }
    }
}

impl FtpLimits {
    /// Construct bounded, non-zero limits.
    pub fn new(
        bandwidth_kb_per_session: u64,
        max_concurrent_connections: u16,
    ) -> Result<Self, FtpError> {
        if bandwidth_kb_per_session == 0 || bandwidth_kb_per_session > 1_073_741_824 {
            return Err(FtpError::Invalid(
                "bandwidth limit must be between 1 KiB and 1 PiB".into(),
            ));
        }
        if max_concurrent_connections == 0 || max_concurrent_connections > 256 {
            return Err(FtpError::Invalid(
                "connection limit must be between 1 and 256".into(),
            ));
        }
        Ok(Self {
            bandwidth_kb_per_session,
            max_concurrent_connections,
        })
    }

    /// Session transfer ceiling in KiB.
    pub fn bandwidth_kb_per_session(self) -> u64 {
        self.bandwidth_kb_per_session
    }

    /// Maximum simultaneous sessions.
    pub fn max_concurrent_connections(self) -> u16 {
        self.max_concurrent_connections
    }

    /// Whether another session may be opened.
    pub fn allows_session(self, active: u16) -> bool {
        active < self.max_concurrent_connections
    }
}

/// A normalized path relative to an FTP account's site root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpPath(PathBuf);

impl FtpPath {
    /// Validate a client-supplied path without touching the filesystem.
    pub fn new(raw: impl AsRef<Path>) -> Result<Self, FtpError> {
        let raw = raw.as_ref();
        if raw.as_os_str().as_encoded_bytes().contains(&0) || raw.is_absolute() {
            return Err(FtpError::Invalid(
                "absolute and NUL-containing paths are forbidden".into(),
            ));
        }
        let mut clean = PathBuf::new();
        for component in raw.components() {
            match component {
                Component::Normal(part) => clean.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(FtpError::Invalid("path traversal is forbidden".into()));
                }
            }
        }
        Ok(Self(clean))
    }

    /// Normalized relative path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// TLS behavior for the supervised FTP listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FtpTlsMode {
    /// Explicit TLS negotiated with `AUTH TLS`.
    StartTls,
    /// TLS negotiated immediately when the connection opens.
    ImplicitTls,
    /// Plain FTP, permitted only on loopback listeners.
    None,
}

impl FromStr for FtpTlsMode {
    type Err = FtpError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "starttls" | "start_tls" => Ok(Self::StartTls),
            "implicittls" | "implicit_tls" => Ok(Self::ImplicitTls),
            "none" => Ok(Self::None),
            _ => Err(FtpError::Invalid("unknown FTP TLS mode".into())),
        }
    }
}

/// FTP credential scoped to exactly one site.
#[derive(Debug, Clone)]
pub struct FtpAccount {
    id: Uuid,
    site_id: Uuid,
    username: String,
    home: PathBuf,
    password: Password,
    read_only: bool,
    limits: FtpLimits,
    enabled: bool,
    last_login_at: Option<DateTime<Utc>>,
    last_login_ip: Option<String>,
    created_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
}

impl FtpAccount {
    /// Create and hash a new per-site FTP credential.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        site_id: Uuid,
        username: impl Into<String>,
        home: PathBuf,
        plaintext: &str,
        read_only: bool,
        limits: FtpLimits,
        now: DateTime<Utc>,
    ) -> Result<Self, FtpError> {
        let username = username.into();
        validate_username(&username)?;
        validate_home(&home)?;
        let password =
            Password::hash(plaintext).map_err(|error| FtpError::Invalid(error.to_string()))?;
        Ok(Self {
            id,
            site_id,
            username,
            home,
            password,
            read_only,
            limits,
            enabled: true,
            last_login_at: None,
            last_login_ip: None,
            created_at: now,
            disabled_at: None,
        })
    }

    /// Restore trusted persisted state.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        site_id: Uuid,
        username: String,
        home: PathBuf,
        password_hash: String,
        read_only: bool,
        limits: FtpLimits,
        enabled: bool,
        last_login_at: Option<DateTime<Utc>>,
        last_login_ip: Option<String>,
        created_at: DateTime<Utc>,
        disabled_at: Option<DateTime<Utc>>,
    ) -> Result<Self, FtpError> {
        validate_username(&username)?;
        validate_home(&home)?;
        Ok(Self {
            id,
            site_id,
            username,
            home,
            password: Password::from_hash(password_hash),
            read_only,
            limits,
            enabled,
            last_login_at,
            last_login_ip,
            created_at,
            disabled_at,
        })
    }

    /// Stable account identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Site that owns this credential.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Username displayed by panel surfaces.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Absolute site root used as the FTP jail.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Encoded password hash for persistence.
    pub fn password_hash(&self) -> &str {
        self.password.hash_str()
    }

    /// Whether mutating filesystem operations are denied.
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Transfer and connection ceilings.
    pub fn limits(&self) -> FtpLimits {
        self.limits
    }

    /// Whether new authentication attempts may succeed.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Time of the most recent successful login.
    pub fn last_login_at(&self) -> Option<DateTime<Utc>> {
        self.last_login_at
    }

    /// Source address of the most recent successful login.
    pub fn last_login_ip(&self) -> Option<&str> {
        self.last_login_ip.as_deref()
    }

    /// Account creation time.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Time at which the account was disabled.
    pub fn disabled_at(&self) -> Option<DateTime<Utc>> {
        self.disabled_at
    }

    /// Verify plaintext against the stored hash.
    pub fn verify_password(&self, plaintext: &str) -> Result<bool, FtpError> {
        self.password
            .verify(plaintext)
            .map_err(|error| FtpError::Internal(error.to_string()))
    }

    /// Whether account state and policy allow one more session.
    pub fn can_open_session(&self, active: u16) -> bool {
        self.enabled && self.limits.allows_session(active)
    }

    /// Disable new logins.
    pub fn disable(&mut self, now: DateTime<Utc>) {
        self.enabled = false;
        self.disabled_at = Some(now);
    }

    /// Enable new logins.
    pub fn enable(&mut self) {
        self.enabled = true;
        self.disabled_at = None;
    }

    /// Replace the password with a newly hashed secret.
    pub fn change_password(&mut self, plaintext: &str) -> Result<(), FtpError> {
        self.password =
            Password::hash(plaintext).map_err(|error| FtpError::Invalid(error.to_string()))?;
        Ok(())
    }

    /// Replace mutable transfer policy fields.
    pub fn change_policy(&mut self, read_only: bool, limits: FtpLimits) {
        self.read_only = read_only;
        self.limits = limits;
    }

    /// Record successful authentication metadata.
    pub fn record_login(&mut self, at: DateTime<Utc>, ip: String) {
        self.last_login_at = Some(at);
        self.last_login_ip = Some(ip);
    }
}

fn validate_username(value: &str) -> Result<(), FtpError> {
    if !(3..=32).contains(&value.len())
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
        || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
    {
        return Err(FtpError::Invalid(
            "username must be 3-32 lowercase ASCII characters and begin with a letter".into(),
        ));
    }
    Ok(())
}

fn validate_home(value: &Path) -> Result<(), FtpError> {
    if !value.is_absolute()
        || value
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(FtpError::Invalid(
            "FTP home must be an absolute normalized site root".into(),
        ));
    }
    Ok(())
}

/// Active session details safe for account management surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FtpSession {
    /// Account using the session.
    pub account_id: Uuid,
    /// Redaction-safe remote IP representation.
    pub remote_ip: String,
    /// Time the session authenticated.
    pub connected_at: DateTime<Utc>,
    /// Completed upload and download bytes.
    pub bytes_transferred: u64,
}

/// Persistence port for FTP accounts.
#[async_trait]
pub trait FtpRepository: Send + Sync + 'static {
    /// Insert an account.
    async fn create(&self, account: &FtpAccount) -> Result<(), RepoError>;
    /// Persist mutable account state.
    async fn update(&self, account: &FtpAccount) -> Result<(), RepoError>;
    /// Delete an account under a site.
    async fn delete(&self, site_id: Uuid, account_id: Uuid) -> Result<bool, RepoError>;
    /// Find an account by site and identifier.
    async fn find(&self, site_id: Uuid, account_id: Uuid) -> Result<Option<FtpAccount>, RepoError>;
    /// Find an account by its site-local username.
    async fn find_by_username(
        &self,
        site_id: Uuid,
        username: &str,
    ) -> Result<Option<FtpAccount>, RepoError>;
    /// List accounts belonging to a site.
    async fn list(&self, site_id: Uuid) -> Result<Vec<FtpAccount>, RepoError>;
}

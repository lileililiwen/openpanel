//! Per-site WebDAV account domain model and persistence port.
//!
//! Mirrors the FTP bounded context: credentials are scoped to one
//! site, jailed to the site root, and every mutating WebDAV verb is
//! denied while the account is read-only.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::ftp::FtpPath;
use crate::{Password, RepoError};

/// WebDAV domain validation, authorization, or persistence error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebDavError {
    /// The caller cannot access the requested site.
    #[error("forbidden")]
    Forbidden,
    /// The requested site does not exist.
    #[error("site not found")]
    SiteNotFound,
    /// The requested account does not exist.
    #[error("WebDAV account not found")]
    NotFound,
    /// The username is already used within the site.
    #[error("WebDAV username already exists for this site")]
    Duplicate,
    /// Input violates a WebDAV invariant.
    #[error("invalid WebDAV value: {0}")]
    Invalid(String),
    /// Authentication was denied without exposing its cause.
    #[error("login incorrect")]
    LoginDenied,
    /// A requested DAV operation is not permitted.
    #[error("WebDAV operation denied")]
    OperationDenied,
    /// The requested resource escapes the account jail.
    #[error("resource not found")]
    OutsideJail,
    /// Persistence or adapter failure.
    #[error("WebDAV operation failed: {0}")]
    Internal(String),
}

/// A jailed WebDAV credential scoped to exactly one site.
#[derive(Debug, Clone)]
pub struct WebDavAccount {
    id: Uuid,
    site_id: Uuid,
    username: String,
    home: PathBuf,
    password: Password,
    read_only: bool,
    enabled: bool,
    last_login_at: Option<DateTime<Utc>>,
    last_login_ip: Option<String>,
    created_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
}

impl WebDavAccount {
    /// Create and hash a new per-site WebDAV credential.
    pub fn new(
        id: Uuid,
        site_id: Uuid,
        username: impl Into<String>,
        home: PathBuf,
        plaintext: &str,
        read_only: bool,
        now: DateTime<Utc>,
    ) -> Result<Self, WebDavError> {
        let username = username.into();
        validate_username(&username)?;
        validate_home(&home)?;
        let password =
            Password::hash(plaintext).map_err(|error| WebDavError::Invalid(error.to_string()))?;
        Ok(Self {
            id,
            site_id,
            username,
            home,
            password,
            read_only,
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
        enabled: bool,
        last_login_at: Option<DateTime<Utc>>,
        last_login_ip: Option<String>,
        created_at: DateTime<Utc>,
        disabled_at: Option<DateTime<Utc>>,
    ) -> Result<Self, WebDavError> {
        validate_username(&username)?;
        validate_home(&home)?;
        Ok(Self {
            id,
            site_id,
            username,
            home,
            password: Password::from_hash(password_hash),
            read_only,
            enabled,
            last_login_at,
            last_login_ip,
            created_at,
            disabled_at,
        })
    }

    /// Stable account identifier — the first DAV path segment.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Site that owns this credential.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Basic-auth username.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Absolute site root used as the DAV jail.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Encoded password hash for persistence.
    pub fn password_hash(&self) -> &str {
        self.password.hash_str()
    }

    /// Whether mutating DAV verbs are denied.
    pub fn read_only(&self) -> bool {
        self.read_only
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
    pub fn verify_password(&self, plaintext: &str) -> Result<bool, WebDavError> {
        self.password
            .verify(plaintext)
            .map_err(|error| WebDavError::Internal(error.to_string()))
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
    pub fn change_password(&mut self, plaintext: &str) -> Result<(), WebDavError> {
        self.password =
            Password::hash(plaintext).map_err(|error| WebDavError::Invalid(error.to_string()))?;
        Ok(())
    }

    /// Replace the mutating-verb policy.
    pub fn change_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// Record successful authentication metadata.
    pub fn record_login(&mut self, at: DateTime<Utc>, ip: String) {
        self.last_login_at = Some(at);
        self.last_login_ip = Some(ip);
    }

    /// Resolve a DAV-relative path inside the jail without touching
    /// the filesystem. Reuses the FTP path validator so traversal and
    /// absolute escapes are rejected identically.
    pub fn resolve(&self, rel: &Path) -> Result<PathBuf, WebDavError> {
        let clean = FtpPath::new(rel).map_err(|error| WebDavError::Invalid(error.to_string()))?;
        Ok(self.home.join(clean.as_path()))
    }
}

fn validate_username(value: &str) -> Result<(), WebDavError> {
    if !(3..=32).contains(&value.len())
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
        || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
    {
        return Err(WebDavError::Invalid(
            "username must be 3-32 lowercase ASCII characters and begin with a letter".into(),
        ));
    }
    Ok(())
}

fn validate_home(value: &Path) -> Result<(), WebDavError> {
    if !value.is_absolute()
        || value
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(WebDavError::Invalid(
            "WebDAV home must be an absolute normalized site root".into(),
        ));
    }
    Ok(())
}

/// Persistence port for WebDAV accounts.
#[async_trait]
pub trait WebDavRepository: Send + Sync + 'static {
    /// Insert an account.
    async fn create(&self, account: &WebDavAccount) -> Result<(), RepoError>;
    /// Persist mutable account state.
    async fn update(&self, account: &WebDavAccount) -> Result<(), RepoError>;
    /// Delete an account under a site.
    async fn delete(&self, site_id: Uuid, account_id: Uuid) -> Result<bool, RepoError>;
    /// Find an account by site and identifier.
    async fn find(
        &self,
        site_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<WebDavAccount>, RepoError>;
    /// Find an account by its globally unique identifier. Used by the
    /// DAV router, where the path carries only the account id.
    async fn find_by_id(&self, account_id: Uuid) -> Result<Option<WebDavAccount>, RepoError>;
    /// Find an account by its site-local username.
    async fn find_by_username(
        &self,
        site_id: Uuid,
        username: &str,
    ) -> Result<Option<WebDavAccount>, RepoError>;
    /// List accounts belonging to a site.
    async fn list(&self, site_id: Uuid) -> Result<Vec<WebDavAccount>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> WebDavAccount {
        WebDavAccount::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "davuser",
            PathBuf::from("/srv/sites/example.com"),
            "correct horse battery",
            false,
            Utc::now(),
        )
        .expect("valid account")
    }

    #[test]
    fn rejects_bad_usernames() {
        for bad in ["ab", "-dash", "Upper", "sp ace"] {
            let result = WebDavAccount::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                bad,
                PathBuf::from("/srv/sites/example.com"),
                "correct horse battery",
                false,
                Utc::now(),
            );
            assert!(
                matches!(result, Err(WebDavError::Invalid(_))),
                "{bad} accepted"
            );
        }
    }

    #[test]
    fn rejects_relative_home() {
        let result = WebDavAccount::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "davuser",
            PathBuf::from("srv/sites/example.com"),
            "correct horse battery",
            false,
            Utc::now(),
        );
        assert!(matches!(result, Err(WebDavError::Invalid(_))));
    }

    #[test]
    fn resolve_stays_inside_jail() {
        let account = account();
        let joined = account
            .resolve(Path::new("a/b/c.txt"))
            .expect("clean relative path");
        assert_eq!(joined, PathBuf::from("/srv/sites/example.com/a/b/c.txt"));
    }

    #[test]
    fn resolve_rejects_traversal_and_absolute() {
        let account = account();
        assert!(matches!(
            account.resolve(Path::new("../etc/passwd")),
            Err(WebDavError::Invalid(_))
        ));
        assert!(matches!(
            account.resolve(Path::new("/etc/passwd")),
            Err(WebDavError::Invalid(_))
        ));
    }

    #[test]
    fn password_roundtrip_and_disable() {
        let mut account = account();
        assert!(account.verify_password("correct horse battery").unwrap());
        assert!(!account.verify_password("wrong secret here").unwrap());
        account.disable(Utc::now());
        assert!(!account.enabled());
        account.enable();
        assert!(account.enabled());
        assert!(account.disabled_at().is_none());
    }
}

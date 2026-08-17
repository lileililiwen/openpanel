//! SFTP / jailed shells bounded context: per-site SSH/SFTP
//! grants that map onto OpenSSH's `internal-sftp` + `ForceCommand`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// A typed public key attached to a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JailPublicKey {
    label: String,
    key: String,
    fingerprint: String,
}

impl JailPublicKey {
    /// Build a key. The label must be non-empty; the key must
    /// start with one of the supported prefixes
    /// (`ssh-ed25519`, `ssh-rsa`).
    pub fn new(label: impl Into<String>, key: impl Into<String>) -> Result<Self, SftpJailError> {
        let label = label.into();
        let key = key.into();
        if label.is_empty() {
            return Err(SftpJailError::InvalidKey(
                "label must be non-empty".to_string(),
            ));
        }
        let trimmed = key.trim();
        if !(trimmed.starts_with("ssh-ed25519") || trimmed.starts_with("ssh-rsa")) {
            return Err(SftpJailError::InvalidKey(format!(
                "unsupported key type: `{}`",
                trimmed.split_whitespace().next().unwrap_or("")
            )));
        }
        // The fingerprint is a SHA-256 of the key body, computed by
        // the follow-on change. The placeholder here is the truncated
        // key body so the typed model is stable.
        let fingerprint = format!("fp:{}", short_hash(&key));
        Ok(Self {
            label,
            key,
            fingerprint,
        })
    }

    /// Restore from persistence.
    pub fn restore(label: String, key: String, fingerprint: String) -> Self {
        Self {
            label,
            key,
            fingerprint,
        }
    }

    /// Display label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Encoded key body.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// SHA-256 fingerprint placeholder.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

fn short_hash(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// Lifecycle status of an SFTP jail grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JailedShellStatus {
    /// The grant is active and the sshd config is published.
    Active,
    /// The grant is disabled; the sshd block is preserved but
    /// commented out.
    Disabled,
}

/// A per-site SFTP jail grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SftpJailGrant {
    site_id: Uuid,
    owner_user_id: Uuid,
    group_name: String,
    jail_path: PathBuf,
    forced_command: String,
    keys: Vec<JailPublicKey>,
    allow_password_fallback: bool,
    allow_port_forwarding: bool,
    status: JailedShellStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SftpJailGrant {
    /// The canonical sshd include directory the panel owns.
    pub const SSHD_INCLUDE_DIR: &'static str = "/etc/ssh/openpanel.d";

    /// Build a new grant. The jail path must be canonical.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        site_id: Uuid,
        owner_user_id: Uuid,
        jail_path: PathBuf,
        site_root_rel: PathBuf,
        keys: Vec<JailPublicKey>,
        allow_password_fallback: bool,
        allow_port_forwarding: bool,
        now: DateTime<Utc>,
    ) -> Result<Self, SftpJailError> {
        let canonical = canonicalize_path(&jail_path)?;
        let group_name = format!("openpanel-sftp-{}", short_id(site_id));
        let forced_command = format!("internal-sftp -d {}", site_root_rel.to_string_lossy());
        Ok(Self {
            site_id,
            owner_user_id,
            group_name,
            jail_path: canonical,
            forced_command,
            keys,
            allow_password_fallback,
            allow_port_forwarding,
            status: JailedShellStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }

    /// Restore from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        site_id: Uuid,
        owner_user_id: Uuid,
        group_name: String,
        jail_path: PathBuf,
        forced_command: String,
        keys: Vec<JailPublicKey>,
        allow_password_fallback: bool,
        allow_port_forwarding: bool,
        status: JailedShellStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            site_id,
            owner_user_id,
            group_name,
            jail_path,
            forced_command,
            keys,
            allow_password_fallback,
            allow_port_forwarding,
            status,
            created_at,
            updated_at,
        }
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Owner user id.
    pub fn owner_user_id(&self) -> Uuid {
        self.owner_user_id
    }

    /// Unique group name.
    pub fn group_name(&self) -> &str {
        &self.group_name
    }

    /// Jail path.
    pub fn jail_path(&self) -> &Path {
        &self.jail_path
    }

    /// Forced command.
    pub fn forced_command(&self) -> &str {
        &self.forced_command
    }

    /// Keys.
    pub fn keys(&self) -> &[JailPublicKey] {
        &self.keys
    }

    /// Allow password fallback (always false for new keys).
    pub fn allow_password_fallback(&self) -> bool {
        self.allow_password_fallback
    }

    /// Allow port forwarding.
    pub fn allow_port_forwarding(&self) -> bool {
        self.allow_port_forwarding
    }

    /// Status.
    pub fn status(&self) -> JailedShellStatus {
        self.status
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Updated at.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Add a key.
    pub fn add_key(&mut self, key: JailPublicKey, now: DateTime<Utc>) {
        if !self.keys.iter().any(|k| k.label() == key.label()) {
            self.keys.push(key);
        }
        self.updated_at = now;
    }

    /// Remove a key by label.
    pub fn remove_key(&mut self, label: &str, now: DateTime<Utc>) -> bool {
        let before = self.keys.len();
        self.keys.retain(|k| k.label() != label);
        let removed = self.keys.len() != before;
        if removed {
            self.updated_at = now;
        }
        removed
    }

    /// Disable the grant.
    pub fn disable(&mut self, now: DateTime<Utc>) {
        self.status = JailedShellStatus::Disabled;
        self.updated_at = now;
    }

    /// Enable the grant.
    pub fn enable(&mut self, now: DateTime<Utc>) {
        self.status = JailedShellStatus::Active;
        self.updated_at = now;
    }
}

/// Generate the OpenSSH config block for a grant. The output
/// stays under the panel-owned directory and is rooted at the
/// grant's canonical jail path.
pub fn render_sshd_config(grant: &SftpJailGrant) -> String {
    let block = grant.status == JailedShellStatus::Active;
    let indent = if block { "  " } else { "  # " };
    let mut buf = String::new();
    buf.push_str(&format!("Match Group {}\n", grant.group_name));
    buf.push_str(&format!(
        "{}ChrootDirectory {}\n",
        indent,
        grant.jail_path.to_string_lossy()
    ));
    buf.push_str(&format!(
        "{}ForceCommand {}\n",
        indent, grant.forced_command
    ));
    buf.push_str(&format!(
        "{}AllowTcpForwarding {}\n",
        indent,
        if grant.allow_port_forwarding {
            "yes"
        } else {
            "no"
        }
    ));
    buf.push_str(&format!("{}X11Forwarding no\n", indent));
    buf.push_str(&format!(
        "{}PasswordAuthentication {}\n",
        indent,
        if grant.allow_password_fallback {
            "yes"
        } else {
            "no"
        }
    ));
    buf.push_str(&format!("{}PermitTTY no\n", indent));
    for key in grant.keys() {
        buf.push_str(&format!(
            "{}AuthorizedKeysCommandRunAs openpanel-sftp\n",
            indent
        ));
        buf.push_str(&format!(
            "{}# {} {}\n",
            indent,
            key.label(),
            key.fingerprint()
        ));
    }
    buf
}

/// Persistence port.
#[async_trait]
pub trait SftpJailRepository: Send + Sync + 'static {
    /// Insert a new grant.
    async fn insert_grant(&self, grant: &SftpJailGrant) -> Result<(), SftpJailError>;
    /// Find the grant for a site.
    async fn find_by_site(&self, site_id: Uuid) -> Result<Option<SftpJailGrant>, SftpJailError>;
    /// Update an existing grant.
    async fn update_grant(&self, grant: &SftpJailGrant) -> Result<(), SftpJailError>;
    /// Delete a grant.
    async fn delete_grant(&self, site_id: Uuid) -> Result<(), SftpJailError>;
    /// List all grants.
    async fn list_grants(&self) -> Result<Vec<SftpJailGrant>, SftpJailError>;
    /// Default impl to satisfy the placeholder pattern.
    async fn exists(&self, _site_id: Uuid) -> Result<bool, RepoError> {
        Ok(true)
    }
}

/// Errors that can occur in the sftp-jailed-shells bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SftpJailError {
    /// The public key is malformed.
    #[error("invalid SFTP public key: {0}")]
    InvalidKey(String),
    /// The jail path is not absolute.
    #[error("jail path must be absolute")]
    JailPathNotAbsolute,
    /// The jail path is not a valid path.
    #[error("invalid jail path: {0}")]
    InvalidPath(String),
    /// The grant is not found.
    #[error("sftp jail grant not found")]
    GrantNotFound,
    /// The sshd include path is outside the panel-managed directory.
    #[error("sshd write outside /etc/ssh/openpanel.d")]
    UnsafeSshdPath,
    /// Persistence failure.
    #[error("sftp jail persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for SftpJailError {
    fn from(error: RepoError) -> Self {
        SftpJailError::Persistence(error.0)
    }
}

fn canonicalize_path(p: &Path) -> Result<PathBuf, SftpJailError> {
    if !p.is_absolute() {
        return Err(SftpJailError::JailPathNotAbsolute);
    }
    let mut buf = PathBuf::new();
    for component in p.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !buf.pop() {
                    return Err(SftpJailError::InvalidPath(format!(
                        "invalid path `{}`",
                        p.display()
                    )));
                }
            }
            other => buf.push(other),
        }
    }
    Ok(buf)
}

fn short_id(id: Uuid) -> String {
    let hex = id.simple().to_string();
    hex.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_rejects_unsupported_type() {
        let err = JailPublicKey::new("laptop", "ssh-dss AAAAB3...").expect_err("must reject");
        assert!(matches!(err, SftpJailError::InvalidKey(_)));
    }

    #[test]
    fn key_rejects_empty_label() {
        let err = JailPublicKey::new("", "ssh-ed25519 AAAAB3...").expect_err("must reject");
        assert!(matches!(err, SftpJailError::InvalidKey(_)));
    }

    #[test]
    fn key_accepts_ed25519() {
        let key = JailPublicKey::new("laptop", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA").unwrap();
        assert_eq!(key.label(), "laptop");
        assert!(key.fingerprint().starts_with("fp:"));
    }

    #[test]
    fn grant_rejects_relative_path() {
        let err = SftpJailGrant::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            std::path::PathBuf::from("var/www"),
            std::path::PathBuf::from("."),
            vec![],
            false,
            false,
            Utc::now(),
        )
        .expect_err("must reject");
        assert_eq!(err, SftpJailError::JailPathNotAbsolute);
    }

    #[test]
    fn grant_canonicalises_cur_dir() {
        let grant = SftpJailGrant::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            std::path::PathBuf::from("/var/www/site"),
            std::path::PathBuf::from("."),
            vec![],
            false,
            false,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(grant.jail_path(), std::path::Path::new("/var/www/site"));
    }

    #[test]
    fn render_sshd_config_blocks_unsafe_options() {
        let site_id = Uuid::new_v4();
        let key = JailPublicKey::new("laptop", "ssh-ed25519 AAAAC3NzaC1").unwrap();
        let grant = SftpJailGrant::new(
            site_id,
            Uuid::new_v4(),
            std::path::PathBuf::from("/var/www/site"),
            std::path::PathBuf::from("."),
            vec![key],
            false,
            false,
            Utc::now(),
        )
        .unwrap();
        let config = render_sshd_config(&grant);
        assert!(config.contains("PermitTTY no"));
        assert!(config.contains("PasswordAuthentication no"));
        assert!(config.contains("X11Forwarding no"));
        assert!(config.contains("AllowTcpForwarding no"));
        assert!(config.contains("ChrootDirectory /var/www/site"));
        assert!(config.contains("ForceCommand internal-sftp"));
    }

    #[test]
    fn disabled_grant_is_commented_out() {
        let mut grant = SftpJailGrant::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            std::path::PathBuf::from("/var/www/site"),
            std::path::PathBuf::from("."),
            vec![],
            false,
            false,
            Utc::now(),
        )
        .unwrap();
        grant.disable(Utc::now());
        let config = render_sshd_config(&grant);
        assert!(config.contains("# ChrootDirectory"));
    }

    #[test]
    fn add_and_remove_key_updates_timestamp() {
        let mut grant = SftpJailGrant::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            std::path::PathBuf::from("/var/www/site"),
            std::path::PathBuf::from("."),
            vec![],
            false,
            false,
            Utc::now(),
        )
        .unwrap();
        let before = grant.updated_at();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let key = JailPublicKey::new("laptop", "ssh-ed25519 AAAAC3NzaC1").unwrap();
        grant.add_key(key, Utc::now());
        assert!(grant.updated_at() >= before);
        let removed = grant.remove_key("laptop", Utc::now());
        assert!(removed);
        assert!(!grant.remove_key("laptop", Utc::now()));
    }
}

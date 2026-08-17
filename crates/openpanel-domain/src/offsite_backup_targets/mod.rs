//! Offsite backup targets bounded context: the `BackupTargetAdapter`
//! contract, encrypted `BackupCredential` records, per-plan
//! `RemoteTargetConfig`, and the `KekRef` wrapper records that let
//! the panel decrypt offsite payloads from a cold backup.
//!
//! The adapter trait is declared here (I/O-free); the concrete S3 /
//! Wasabi / B2 / rsync adapters and the KekManager crypto live in
//! the application layer.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Storage families a backup credential can authenticate to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// AWS S3 or any S3-compatible endpoint (MinIO, etc.).
    S3,
    /// Wasabi (S3-compatible).
    Wasabi,
    /// Backblaze B2.
    B2,
    /// rsync over SSH.
    Rsync,
}

impl CredentialKind {
    /// Wire name used by the REST / CLI adapters.
    pub fn as_str(&self) -> &'static str {
        match self {
            CredentialKind::S3 => "s3",
            CredentialKind::Wasabi => "wasabi",
            CredentialKind::B2 => "b2",
            CredentialKind::Rsync => "rsync",
        }
    }
}

/// A stored offsite credential. The `secret` is opaque encrypted
/// bytes (AES-256-GCM under the derived KEK); the panel never
/// stores or echoes the plaintext access key / secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupCredential {
    id: Uuid,
    kind: CredentialKind,
    label: String,
    /// `nonce_hex:ciphertext_hex` under the KEK.
    secret_enc: String,
    created_at: DateTime<Utc>,
}

impl BackupCredential {
    /// Build a new credential record. The label must be non-empty
    /// and ≤ 64 chars; the encrypted secret must be non-empty.
    pub fn new(
        id: Uuid,
        kind: CredentialKind,
        label: impl Into<String>,
        secret_enc: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, OffsiteBackupError> {
        let label = label.into();
        if label.is_empty() || label.len() > 64 {
            return Err(OffsiteBackupError::InvalidLabel(format!(
                "label must be 1..=64 chars, got {}",
                label.len()
            )));
        }
        let secret_enc = secret_enc.into();
        if secret_enc.is_empty() {
            return Err(OffsiteBackupError::InvalidSecret(
                "encrypted secret must not be empty".to_string(),
            ));
        }
        Ok(Self {
            id,
            kind,
            label,
            secret_enc,
            created_at,
        })
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Storage family.
    pub fn kind(&self) -> CredentialKind {
        self.kind
    }

    /// Display label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The encrypted secret payload.
    pub fn secret_enc(&self) -> &str {
        &self.secret_enc
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Whether this credential is usable for the given kind. The
    /// application layer verifies the decrypted payload fields.
    pub fn matches_kind(&self, kind: CredentialKind) -> bool {
        self.kind == kind
    }
}

/// Per-plan remote destination configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTargetConfig {
    plan_id: Uuid,
    credential_id: Uuid,
    /// Object-key prefix inside the target (e.g. `host1/backups/`).
    prefix: String,
    /// Optional cron schedule for the offsite copy.
    schedule: Option<String>,
}

impl RemoteTargetConfig {
    /// Build a remote target config. The prefix must be non-empty
    /// and must not contain a leading slash.
    pub fn new(
        plan_id: Uuid,
        credential_id: Uuid,
        prefix: impl Into<String>,
        schedule: Option<String>,
    ) -> Result<Self, OffsiteBackupError> {
        let prefix = prefix.into();
        if prefix.is_empty() || prefix.len() > 1024 {
            return Err(OffsiteBackupError::InvalidPrefix(format!(
                "prefix must be 1..=1024 chars, got {}",
                prefix.len()
            )));
        }
        if prefix.starts_with('/') {
            return Err(OffsiteBackupError::InvalidPrefix(
                "prefix must not start with a slash".to_string(),
            ));
        }
        Ok(Self {
            plan_id,
            credential_id,
            prefix,
            schedule,
        })
    }

    /// Plan this config attaches to.
    pub fn plan_id(&self) -> Uuid {
        self.plan_id
    }

    /// Credential used to authenticate.
    pub fn credential_id(&self) -> Uuid {
        self.credential_id
    }

    /// Object-key prefix.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Optional cron schedule.
    pub fn schedule(&self) -> Option<&str> {
        self.schedule.as_deref()
    }

    /// Compose the object key for an artifact under this target.
    pub fn object_key(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_string()
        } else if self.prefix.ends_with('/') {
            format!("{}{}", self.prefix, name)
        } else {
            format!("{}/{}", self.prefix, name)
        }
    }
}

/// A reference to a wrapped KEK stored in `backup_kek_wrappers`.
/// The panel wraps a per-install KEK with the master key so it can
/// decrypt without the operator passphrase; a cold restore can
/// re-derive the KEK from the passphrase + salt alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KekRef {
    id: Uuid,
    /// Argon2id salt (hex) used to derive the KEK.
    salt_hex: String,
    /// Master-key-wrapped KEK bytes (hex).
    wrapped_kek_hex: String,
    created_at: DateTime<Utc>,
}

impl KekRef {
    /// Build a KEK wrapper record.
    pub fn new(
        id: Uuid,
        salt_hex: impl Into<String>,
        wrapped_kek_hex: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, OffsiteBackupError> {
        let salt_hex = salt_hex.into();
        let wrapped_kek_hex = wrapped_kek_hex.into();
        if salt_hex.is_empty() || wrapped_kek_hex.is_empty() {
            return Err(OffsiteBackupError::InvalidKek(
                "salt and wrapped KEK must not be empty".to_string(),
            ));
        }
        Ok(Self {
            id,
            salt_hex,
            wrapped_kek_hex,
            created_at,
        })
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Argon2id salt.
    pub fn salt_hex(&self) -> &str {
        &self.salt_hex
    }

    /// Master-key-wrapped KEK.
    pub fn wrapped_kek_hex(&self) -> &str {
        &self.wrapped_kek_hex
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// Storage adapter contract. Each concrete adapter maps the
/// operations onto its SDK; `test` probes reachability and
/// permissions without writing data.
#[async_trait]
pub trait BackupTargetAdapter: Send + Sync + 'static {
    /// Type-erased error for the adapter.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Upload `bytes` to `key`. Overwrites existing objects.
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    /// Download the object at `key`.
    async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error>;
    /// List object keys under `prefix`.
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error>;
    /// Delete the object at `key`. Deleting a missing key is not
    /// an error.
    async fn delete(&self, key: &str) -> Result<(), Self::Error>;
    /// Probe reachability and credentials.
    async fn test(&self) -> Result<(), Self::Error>;
}

/// Persistence port for offsite credentials, target configs, and
/// KEK wrappers.
#[async_trait]
pub trait OffsiteBackupRepository: Send + Sync + 'static {
    /// Insert a credential.
    async fn insert_credential(
        &self,
        credential: &BackupCredential,
    ) -> Result<(), OffsiteBackupError>;
    /// Find a credential by id.
    async fn find_credential(
        &self,
        id: Uuid,
    ) -> Result<Option<BackupCredential>, OffsiteBackupError>;
    /// List all credentials.
    async fn list_credentials(&self) -> Result<Vec<BackupCredential>, OffsiteBackupError>;
    /// Delete a credential by id. The implementation MUST refuse
    /// when the credential is attached to a plan's remote config.
    async fn delete_credential(&self, id: Uuid) -> Result<(), OffsiteBackupError>;
    /// Insert a remote target config.
    async fn insert_remote_target(
        &self,
        config: &RemoteTargetConfig,
    ) -> Result<(), OffsiteBackupError>;
    /// Load the remote target config for a plan, if any.
    async fn remote_target_for_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<Option<RemoteTargetConfig>, OffsiteBackupError>;
    /// Insert a KEK wrapper.
    async fn insert_kek(&self, kek: &KekRef) -> Result<(), OffsiteBackupError>;
    /// Load the current KEK wrapper.
    async fn current_kek(&self) -> Result<Option<KekRef>, OffsiteBackupError>;
}

/// Errors that can occur in the offsite-backup-targets bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OffsiteBackupError {
    /// The credential label is invalid.
    #[error("invalid credential label: {0}")]
    InvalidLabel(String),
    /// The encrypted secret is malformed.
    #[error("invalid encrypted secret: {0}")]
    InvalidSecret(String),
    /// The remote target prefix is invalid.
    #[error("invalid remote target prefix: {0}")]
    InvalidPrefix(String),
    /// The KEK wrapper record is malformed.
    #[error("invalid KEK record: {0}")]
    InvalidKek(String),
    /// The credential is still attached to a plan and cannot be
    /// deleted.
    #[error("credential {0} is in use by a plan")]
    CredentialInUse(Uuid),
    /// The credential was not found.
    #[error("credential not found")]
    CredentialNotFound,
    /// Decryption of the credential secret failed.
    #[error("credential decryption failed")]
    CredentialDecrypt,
    /// The adapter reported a remote error.
    #[error("remote adapter error: {0}")]
    Adapter(String),
    /// Persistence layer failure.
    #[error("offsite backup persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for OffsiteBackupError {
    fn from(error: RepoError) -> Self {
        OffsiteBackupError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(kind: CredentialKind) -> BackupCredential {
        BackupCredential::new(Uuid::new_v4(), kind, "prod-bucket", "aabb:ccdd", Utc::now()).unwrap()
    }

    #[test]
    fn credential_rejects_empty_label() {
        let err = BackupCredential::new(
            Uuid::new_v4(),
            CredentialKind::S3,
            "",
            "aabb:ccdd",
            Utc::now(),
        )
        .expect_err("must reject");
        assert!(matches!(err, OffsiteBackupError::InvalidLabel(_)));
    }

    #[test]
    fn credential_rejects_empty_secret() {
        let err = BackupCredential::new(Uuid::new_v4(), CredentialKind::S3, "prod", "", Utc::now())
            .expect_err("must reject");
        assert!(matches!(err, OffsiteBackupError::InvalidSecret(_)));
    }

    #[test]
    fn credential_matches_kind() {
        let cred = credential(CredentialKind::S3);
        assert!(cred.matches_kind(CredentialKind::S3));
        assert!(!cred.matches_kind(CredentialKind::B2));
    }

    #[test]
    fn remote_target_composes_object_key() {
        let config =
            RemoteTargetConfig::new(Uuid::new_v4(), Uuid::new_v4(), "host1/backups", None).unwrap();
        assert_eq!(
            config.object_key("site.tar.gz"),
            "host1/backups/site.tar.gz"
        );
        let config =
            RemoteTargetConfig::new(Uuid::new_v4(), Uuid::new_v4(), "host1/backups/", None)
                .unwrap();
        assert_eq!(
            config.object_key("site.tar.gz"),
            "host1/backups/site.tar.gz"
        );
    }

    #[test]
    fn remote_target_rejects_leading_slash() {
        let err = RemoteTargetConfig::new(Uuid::new_v4(), Uuid::new_v4(), "/etc", None)
            .expect_err("must reject");
        assert!(matches!(err, OffsiteBackupError::InvalidPrefix(_)));
    }

    #[test]
    fn kek_ref_rejects_empty_salt() {
        let err = KekRef::new(Uuid::new_v4(), "", "abcd", Utc::now()).expect_err("must reject");
        assert!(matches!(err, OffsiteBackupError::InvalidKek(_)));
    }

    #[test]
    fn credential_kind_wire_names() {
        assert_eq!(CredentialKind::S3.as_str(), "s3");
        assert_eq!(CredentialKind::Wasabi.as_str(), "wasabi");
        assert_eq!(CredentialKind::B2.as_str(), "b2");
        assert_eq!(CredentialKind::Rsync.as_str(), "rsync");
    }
}

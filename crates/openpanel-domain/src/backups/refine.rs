//! Backup refinement: `ResourceKind`, `RestoreScope`, and
//! `BackupTargetKind` value objects that the per-resource
//! restore and the remote-target policy depend on.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// The kind of resource a backup plan can address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// A nginx-hosted site.
    Site,
    /// A managed database.
    Database,
    /// A mail domain (mailboxes + aliases).
    MailDomain,
    /// A single mailbox.
    Mailbox,
    /// The audit log (full-database dump).
    AuditLog,
    /// OpenPanel configuration (the small YAML store under
    /// `/etc/openpanel`).
    Configuration,
}

/// A typed restore scope. The selector is a kind-specific
/// `serde_json::Value` so the application layer can validate
/// the presence of the per-kind fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreScope {
    /// The kind of resource to restore.
    pub kind: ResourceKind,
    /// Kind-specific selector fields (a JSON object).
    pub selector: serde_json::Value,
}

impl RestoreScope {
    /// Build a new scope. The selector MUST be a JSON object so
    /// the application layer can validate per-kind fields.
    pub fn new(kind: ResourceKind, selector: serde_json::Value) -> Result<Self, BackupRefineError> {
        if !selector.is_object() {
            return Err(BackupRefineError::InvalidSelector(format!(
                "selector must be a JSON object, got {}",
                selector_type(&selector)
            )));
        }
        Ok(Self { kind, selector })
    }

    /// Read a selector field by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.selector.get(key)
    }

    /// Require a non-empty selector field by key.
    pub fn require_str(&self, key: &str) -> Result<&str, BackupRefineError> {
        let value = self
            .get(key)
            .ok_or_else(|| BackupRefineError::MissingSelectorField(key.to_string()))?;
        value
            .as_str()
            .ok_or_else(|| BackupRefineError::InvalidSelectorField(key.to_string()))
    }
}

/// Where a backup plan stores its artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupTargetKind {
    /// Local filesystem (default).
    Local,
    /// Off-site S3 (AWS S3, MinIO, etc.).
    OffsiteS3,
    /// Off-site rsync over SSH.
    OffsiteRsync,
    /// Off-site Backblaze B2.
    OffsiteB2,
    /// Off-site Wasabi.
    OffsiteWasabi,
}

/// The remote-target policy record attached to a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupTargetPolicy {
    /// The storage target kind.
    pub kind: BackupTargetKind,
    /// Optional S3 bucket name (validated by the application
    /// layer when `kind == OffsiteS3`).
    pub bucket: Option<String>,
    /// Optional rsync URL (validated when `kind == OffsiteRsync`).
    pub rsync_url: Option<String>,
}

impl BackupTargetPolicy {
    /// Build a local target policy.
    pub fn local() -> Self {
        Self {
            kind: BackupTargetKind::Local,
            bucket: None,
            rsync_url: None,
        }
    }

    /// Build an off-site S3 target policy.
    pub fn s3(bucket: impl Into<String>) -> Result<Self, BackupRefineError> {
        let bucket = bucket.into();
        if bucket.is_empty() {
            return Err(BackupRefineError::InvalidSelector(
                "S3 bucket must be non-empty".to_string(),
            ));
        }
        Ok(Self {
            kind: BackupTargetKind::OffsiteS3,
            bucket: Some(bucket),
            rsync_url: None,
        })
    }

    /// Build an off-site rsync target policy.
    pub fn rsync(url: impl Into<String>) -> Result<Self, BackupRefineError> {
        let url = url.into();
        if url.is_empty() {
            return Err(BackupRefineError::InvalidSelector(
                "rsync URL must be non-empty".to_string(),
            ));
        }
        Ok(Self {
            kind: BackupTargetKind::OffsiteRsync,
            bucket: None,
            rsync_url: Some(url),
        })
    }

    /// The display label used by the web UI.
    pub fn label(&self) -> &'static str {
        match self.kind {
            BackupTargetKind::Local => "local",
            BackupTargetKind::OffsiteS3 => "offsite-s3",
            BackupTargetKind::OffsiteRsync => "offsite-rsync",
            BackupTargetKind::OffsiteB2 => "offsite-b2",
            BackupTargetKind::OffsiteWasabi => "offsite-wasabi",
        }
    }
}

/// Errors that can occur in the backup refinement.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BackupRefineError {
    /// The selector is malformed.
    #[error("invalid restore scope: {0}")]
    InvalidSelector(String),
    /// A required selector field is missing.
    #[error("missing required selector field: {0}")]
    MissingSelectorField(String),
    /// A selector field is the wrong type.
    #[error("invalid selector field: {0}")]
    InvalidSelectorField(String),
}

fn selector_type(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// A typed restore identifier that combines the run id with the
/// resource scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// The id of the run this restore belongs to.
    pub run_id: Uuid,
    /// The typed resource scope to restore.
    pub scope: RestoreScope,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_scope_rejects_non_object_selector() {
        let err = RestoreScope::new(ResourceKind::Site, serde_json::json!("not-an-object"))
            .expect_err("must reject");
        assert!(matches!(err, BackupRefineError::InvalidSelector(_)));
    }

    #[test]
    fn restore_scope_accepts_object_selector() {
        let scope =
            RestoreScope::new(ResourceKind::Site, serde_json::json!({"site_id": "abc"})).unwrap();
        assert_eq!(scope.require_str("site_id").unwrap(), "abc");
    }

    #[test]
    fn restore_scope_missing_field_is_error() {
        let scope = RestoreScope::new(ResourceKind::Site, serde_json::json!({})).unwrap();
        let err = scope.require_str("site_id").expect_err("must reject");
        assert!(matches!(err, BackupRefineError::MissingSelectorField(_)));
    }

    #[test]
    fn s3_target_rejects_empty_bucket() {
        let err = BackupTargetPolicy::s3("").expect_err("must reject");
        assert!(matches!(err, BackupRefineError::InvalidSelector(_)));
    }

    #[test]
    fn rsync_target_rejects_empty_url() {
        let err = BackupTargetPolicy::rsync("").expect_err("must reject");
        assert!(matches!(err, BackupRefineError::InvalidSelector(_)));
    }

    #[test]
    fn target_label_matches_kind() {
        assert_eq!(BackupTargetPolicy::local().label(), "local");
        assert_eq!(
            BackupTargetPolicy::s3("bucket").unwrap().label(),
            "offsite-s3"
        );
        assert_eq!(
            BackupTargetPolicy::rsync("rsync://x").unwrap().label(),
            "offsite-rsync"
        );
    }
}

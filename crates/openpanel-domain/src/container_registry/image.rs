//! Stored image, image digest, and scan status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// OCI image digest (`sha256:` + lowercase hex). Validated on
/// construction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageDigest(String);

impl ImageDigest {
    /// Construct an image digest; rejects malformed values.
    pub fn new(value: impl Into<String>) -> Result<Self, super::error::RegistryError> {
        let s = value.into();
        if !s.starts_with("sha256:")
            || s.len() != 71
            || !s[7..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(super::error::RegistryError::ImageNotFound(s));
        }
        Ok(Self(s.to_ascii_lowercase()))
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ImageDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for ImageDigest {
    type Err = super::error::RegistryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Per-image scan status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanStatus {
    /// No scan has been recorded yet.
    Pending,
    /// A scan finished without findings.
    Clean,
    /// A scan finished with at least one finding.
    WithFindings,
    /// The scan failed (e.g. scanner unavailable).
    Failed,
}

impl ScanStatus {
    /// Stable string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanStatus::Pending => "pending",
            ScanStatus::Clean => "clean",
            ScanStatus::WithFindings => "with_findings",
            ScanStatus::Failed => "failed",
        }
    }
}

/// A stored image in a namespace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredImage {
    /// Image digest.
    pub digest: ImageDigest,
    /// Owning namespace.
    pub namespace: super::namespace::NamespaceId,
    /// On-disk size in bytes.
    pub size_bytes: u64,
    /// When the image was first pushed.
    pub pushed_at: DateTime<Utc>,
    /// Reference (tag) — informational, not enforced unique.
    pub reference: Option<String>,
    /// Current scan status.
    pub scan_status: ScanStatus,
}

impl StoredImage {
    /// Construct a freshly-pushed image with `Pending` scan status.
    pub fn new_pushed(
        digest: ImageDigest,
        namespace: super::namespace::NamespaceId,
        size_bytes: u64,
        reference: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            digest,
            namespace,
            size_bytes,
            pushed_at: now,
            reference,
            scan_status: ScanStatus::Pending,
        }
    }

    /// Convenience for tests.
    #[allow(dead_code)]
    pub fn with_actor(_actor: Uuid) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container_registry::namespace::NamespaceId;

    #[test]
    fn digest_validates_format() {
        let hex = "a".repeat(64);
        let valid = format!("sha256:{hex}");
        assert!(ImageDigest::new(valid).is_ok());
        assert!(ImageDigest::new("sha256:zz").is_err());
        assert!(ImageDigest::new("md5:abc").is_err());
    }

    #[test]
    fn image_starts_pending() {
        let ns = NamespaceId::new("alpha").unwrap();
        let digest = ImageDigest::new(format!("sha256:{}", "a".repeat(64))).unwrap();
        let img = StoredImage::new_pushed(digest, ns, 1024, Some("latest".into()), Utc::now());
        assert_eq!(img.scan_status, ScanStatus::Pending);
    }
}
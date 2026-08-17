//! Per-user image namespace.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable namespace identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamespaceId(String);

impl NamespaceId {
    /// Construct a namespace id; rejects empty / over-long / bad-
    /// character values.
    pub fn new(value: impl Into<String>) -> Result<Self, super::error::RegistryError> {
        let s = value.into();
        if s.is_empty()
            || s.len() > 64
            || !s.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
            })
        {
            return Err(super::error::RegistryError::NamespaceNotFound(s));
        }
        Ok(Self(s))
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for NamespaceId {
    type Err = super::error::RegistryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Per-user namespace: an isolated image-storage partition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageNamespace {
    /// Stable namespace id.
    pub namespace_id: NamespaceId,
    /// Owning user (tenant isolation).
    pub owner: Uuid,
    /// Quota in bytes.
    pub quota_bytes: u64,
    /// Currently used bytes.
    pub used_bytes: u64,
    /// When the namespace was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ImageNamespace {
    /// Construct a fresh namespace.
    pub fn new(
        namespace_id: NamespaceId,
        owner: Uuid,
        quota_bytes: u64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            namespace_id,
            owner,
            quota_bytes,
            used_bytes: 0,
            created_at: now,
        }
    }

    /// Returns `true` if adding `delta` bytes would exceed the
    /// namespace quota.
    pub fn would_exceed(&self, delta: u64) -> bool {
        self.used_bytes.saturating_add(delta) > self.quota_bytes
    }

    /// Bookkeeping: add `delta` bytes to the used total.
    pub fn account_push(&mut self, delta: u64) {
        self.used_bytes = self.used_bytes.saturating_add(delta);
    }

    /// Bookkeeping: remove `bytes` bytes from the used total
    /// (after a successful prune).
    pub fn account_delete(&mut self, bytes: u64) {
        self.used_bytes = self.used_bytes.saturating_sub(bytes);
    }

    /// Returns `true` if the namespace has any capacity at all.
    pub fn has_capacity(&self) -> bool {
        self.used_bytes < self.quota_bytes
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn namespace_id_validates() {
        assert!(NamespaceId::new("alpha").is_ok());
        assert!(NamespaceId::new("Beta").is_err());
        assert!(NamespaceId::new("").is_err());
    }

    #[test]
    fn namespace_quota_blocks_overflow() {
        let owner = Uuid::new_v4();
        let mut ns =
            ImageNamespace::new(NamespaceId::new("alpha").unwrap(), owner, 100, Utc::now());
        assert!(!ns.would_exceed(50));
        ns.account_push(50);
        assert!(ns.would_exceed(60));
        assert!(!ns.would_exceed(50));
    }
}

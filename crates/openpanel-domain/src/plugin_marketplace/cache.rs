//! Catalog cache abstraction.
//!
//! The marketplace layer caches verified catalogs locally so
//! repeated installs do not require a network round-trip and so the
//! panel can operate offline. The cache is key/value: `(digest ->
//! snapshot)`. Adapters live in `openpanel-app`.

use chrono::{DateTime, Utc};

use super::catalog::MarketplaceCatalog;
use crate::common::error::RepoError;

/// Snapshot persisted in the cache.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogSnapshot {
    /// Verified catalog.
    pub catalog: MarketplaceCatalog,
    /// When the snapshot was first written to the cache.
    pub cached_at: DateTime<Utc>,
}

/// Catalog cache trait. Implementations may be SQLite, a file, or
/// in-memory for tests.
#[async_trait::async_trait]
pub trait CatalogCache: Send + Sync {
    /// Look up a snapshot by digest.
    async fn get(&self, digest: &str) -> Result<Option<CatalogSnapshot>, RepoError>;

    /// Persist a snapshot; overwrites any existing snapshot with
    /// the same digest.
    async fn put(&self, snapshot: &CatalogSnapshot) -> Result<(), RepoError>;

    /// Drop a snapshot by digest.
    async fn drop(&self, digest: &str) -> Result<(), RepoError>;

    /// Return the most recently cached snapshot, if any.
    async fn latest(&self) -> Result<Option<CatalogSnapshot>, RepoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_holds_catalog_and_timestamp() {
        let snapshot = CatalogSnapshot {
            catalog: MarketplaceCatalog {
                digest: "abc".into(),
                entries: Vec::new(),
                publisher_id: "publisher".into(),
                expires_at: 0,
                fetched_at: None,
            },
            cached_at: Utc::now(),
        };
        assert_eq!(snapshot.catalog.digest, "abc");
    }
}

//! SQLite-backed `CatalogCache` adapter.

use async_trait::async_trait;
use chrono::Utc;
use openpanel_domain::{
    CatalogCache, CatalogSnapshot, MarketplaceCatalog, common::error::RepoError,
};
use sqlx::{Pool, Sqlite};

/// SQLite-backed `CatalogCache` adapter. Caches verified catalogs by
/// digest so a tampered catalog cannot overwrite a verified one.
#[derive(Clone)]
pub struct SqliteCatalogCache {
    pool: Pool<Sqlite>,
}

impl SqliteCatalogCache {
    /// Construct a new cache bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CatalogCache for SqliteCatalogCache {
    async fn get(&self, digest: &str) -> Result<Option<CatalogSnapshot>, RepoError> {
        let row: Option<CacheRow> = sqlx::query_as(
            "SELECT digest, publisher_id, expires_at, payload, cached_at
             FROM marketplace_catalog_cache WHERE digest = ?",
        )
        .bind(digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let cached_at = row.cached_at;
        let catalog = row.into_catalog().map_err(RepoError::new)?;
        Ok(Some(CatalogSnapshot { catalog, cached_at }))
    }

    async fn put(&self, snapshot: &CatalogSnapshot) -> Result<(), RepoError> {
        #[derive(serde::Serialize)]
        struct StoredCatalog<'a> {
            entries: &'a [openpanel_domain::MarketplacePlugin],
        }
        let payload = serde_json::to_string(&StoredCatalog {
            entries: &snapshot.catalog.entries,
        })
        .map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO marketplace_catalog_cache
                (digest, publisher_id, expires_at, payload, cached_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(snapshot.catalog.digest.clone())
        .bind(snapshot.catalog.publisher_id.clone())
        .bind(snapshot.catalog.expires_at as i64)
        .bind(payload)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn drop(&self, digest: &str) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM marketplace_catalog_cache WHERE digest = ?")
            .bind(digest)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn latest(&self) -> Result<Option<CatalogSnapshot>, RepoError> {
        let row: Option<CacheRow> = sqlx::query_as(
            "SELECT digest, publisher_id, expires_at, payload, cached_at
             FROM marketplace_catalog_cache ORDER BY cached_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let cached_at = row.cached_at;
        let catalog = row.into_catalog().map_err(RepoError::new)?;
        Ok(Some(CatalogSnapshot { catalog, cached_at }))
    }
}

#[derive(sqlx::FromRow)]
struct CacheRow {
    digest: String,
    publisher_id: String,
    expires_at: i64,
    payload: String,
    cached_at: chrono::DateTime<Utc>,
}

impl CacheRow {
    fn into_catalog(self) -> Result<MarketplaceCatalog, String> {
        #[derive(serde::Deserialize)]
        struct StoredCatalog {
            entries: Vec<openpanel_domain::MarketplacePlugin>,
        }
        let stored: StoredCatalog =
            serde_json::from_str(&self.payload).map_err(|e| format!("decode: {e}"))?;
        Ok(MarketplaceCatalog {
            digest: self.digest,
            entries: stored.entries,
            publisher_id: self.publisher_id,
            expires_at: self.expires_at as u64,
            fetched_at: None,
        })
    }
}

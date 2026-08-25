//! SQLite site HTTP-controls repository adapter.

use async_trait::async_trait;
use openpanel_domain::{
    RepoError,
    site_http_controls::{SiteHttpControls, SiteHttpRepository},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed controls repository.
pub struct SqliteSiteHttpRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSiteHttpRepository {
    /// Construct the adapter over a SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SiteHttpRepository for SqliteSiteHttpRepository {
    async fn get(&self, site_id: Uuid) -> Result<Option<SiteHttpControls>, RepoError> {
        let document = sqlx::query_scalar::<_, String>(
            "SELECT document_json FROM site_http_controls WHERE site_id = ?",
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(repo_error)?;
        document
            .map(|json| {
                serde_json::from_str::<SiteHttpControls>(&json)
                    .map_err(repo_error)
                    .and_then(|controls| controls.validated().map_err(repo_error))
            })
            .transpose()
    }

    async fn put(&self, controls: &SiteHttpControls) -> Result<(), RepoError> {
        let json = serde_json::to_string(controls).map_err(repo_error)?;
        sqlx::query(
            "INSERT INTO site_http_controls (site_id, document_json, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT(site_id) DO UPDATE SET document_json = excluded.document_json, \
             updated_at = excluded.updated_at",
        )
        .bind(controls.site_id().to_string())
        .bind(json)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }
}

fn repo_error(error: impl std::fmt::Display) -> RepoError {
    RepoError::new(error.to_string())
}

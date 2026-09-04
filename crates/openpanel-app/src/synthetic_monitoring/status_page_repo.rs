use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use openpanel_domain::{
    RepoError,
    synthetic_monitoring::{Slug, StatusEntry, StatusPage, StatusPageError, StatusPageRepository},
};

/// SQLite-backed single-row status page repository.
#[derive(Clone)]
pub struct SqliteStatusPageRepository {
    pool: SqlitePool,
}

impl SqliteStatusPageRepository {
    /// Construct over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl StatusPageRepository for SqliteStatusPageRepository {
    async fn save(&self, page: &StatusPage) -> Result<(), StatusPageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
        sqlx::query("DELETE FROM status_page WHERE id = 1")
            .execute(&mut *tx)
            .await
            .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
        sqlx::query(
            "INSERT INTO status_page (id, slug, enabled, updated_at) \
             VALUES (1, ?, ?, ?)",
        )
        .bind(page.slug.as_str())
        .bind(i64::from(page.enabled))
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| StatusPageError::Persistence(e.to_string()))?;

        sqlx::query("DELETE FROM status_page_entries")
            .execute(&mut *tx)
            .await
            .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
        for entry in &page.entries {
            sqlx::query("INSERT INTO status_page_entries (check_id, label) VALUES (?, ?)")
                .bind(entry.check_id.to_string())
                .bind(&entry.label)
                .execute(&mut *tx)
                .await
                .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn load(&self) -> Result<StatusPage, StatusPageError> {
        let row = sqlx::query("SELECT slug, enabled FROM status_page WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
        let Some(row) = row else {
            return Ok(StatusPage::empty(Slug::new("default")?));
        };
        let slug: String = row.try_get("slug").map_err(map_sqlx)?;
        let enabled: i64 = row.try_get("enabled").map_err(map_sqlx)?;
        let entries = sqlx::query("SELECT check_id, label FROM status_page_entries")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StatusPageError::Persistence(e.to_string()))?
            .into_iter()
            .map(|row| {
                let id: String = row.try_get("check_id").map_err(map_sqlx)?;
                let label: String = row.try_get("label").map_err(map_sqlx)?;
                let check_id = Uuid::parse_str(&id)
                    .map_err(|e| StatusPageError::Persistence(e.to_string()))?;
                Ok::<_, StatusPageError>(StatusEntry { check_id, label })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StatusPage {
            slug: Slug::new(slug)
                .map_err(|_| StatusPageError::Persistence("invalid slug".into()))?,
            enabled: enabled != 0,
            entries,
        })
    }
}

fn map_sqlx(error: sqlx::Error) -> StatusPageError {
    StatusPageError::Persistence(error.to_string())
}

#[doc(hidden)]
pub fn _unused(_e: RepoError) {}

//! SQLite adapter for preview environments.

use chrono::{DateTime, Utc};
use openpanel_domain::{PreviewEnvironment, PreviewRepository, PreviewState, RepoError};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `PreviewRepository`.
#[derive(Clone)]
pub struct SqlitePreviewRepository {
    pool: SqlitePool,
}

impl SqlitePreviewRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PreviewRepository for SqlitePreviewRepository {
    async fn save(&self, preview: &PreviewEnvironment) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO previews \
             (id, site_id, repo_id, pr_number, state, hostname, created_at, ready_at, \
              expires_at, destroyed_at, destroy_reason) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(preview.id().to_string())
        .bind(preview.site_id().to_string())
        .bind(preview.repo_id().to_string())
        .bind(i64::from(preview.pr_number()))
        .bind(preview.state().as_str())
        .bind(preview.hostname())
        .bind(preview.created_at().to_rfc3339())
        .bind(preview.ready_at().map(|t| t.to_rfc3339()))
        .bind(preview.expires_at().map(|t| t.to_rfc3339()))
        .bind(preview.destroyed_at().map(|t| t.to_rfc3339()))
        .bind(preview.destroy_reason())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<PreviewEnvironment>, RepoError> {
        let row = sqlx::query("SELECT * FROM previews WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode).transpose()
    }

    async fn list_live(&self, repo_id: Uuid) -> Result<Vec<PreviewEnvironment>, RepoError> {
        let rows = sqlx::query(
            "SELECT * FROM previews WHERE repo_id = ? AND state != 'destroyed' \
             ORDER BY created_at DESC",
        )
        .bind(repo_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode).collect()
    }

    async fn list_for_site(&self, site_id: Uuid) -> Result<Vec<PreviewEnvironment>, RepoError> {
        let rows = sqlx::query("SELECT * FROM previews WHERE site_id = ? ORDER BY created_at DESC")
            .bind(site_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode).collect()
    }

    async fn list_expired(&self, now: DateTime<Utc>) -> Result<Vec<PreviewEnvironment>, RepoError> {
        let rows = sqlx::query(
            "SELECT * FROM previews WHERE state = 'ready' AND expires_at IS NOT NULL \
             AND expires_at <= ?",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode).collect()
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM previews WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }
}

fn decode(row: sqlx::sqlite::SqliteRow) -> Result<PreviewEnvironment, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let repo_id: String = row.try_get("repo_id").map_err(map_sqlx)?;
    let pr_number: i64 = row.try_get("pr_number").map_err(map_sqlx)?;
    let state: String = row.try_get("state").map_err(map_sqlx)?;
    let hostname: String = row.try_get("hostname").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let ready_at: Option<String> = row.try_get("ready_at").map_err(map_sqlx)?;
    let expires_at: Option<String> = row.try_get("expires_at").map_err(map_sqlx)?;
    let destroyed_at: Option<String> = row.try_get("destroyed_at").map_err(map_sqlx)?;
    let destroy_reason: Option<String> = row.try_get("destroy_reason").map_err(map_sqlx)?;

    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let repo_id = Uuid::parse_str(&repo_id).map_err(|e| RepoError::new(e.to_string()))?;
    let state = match state.as_str() {
        "creating" => PreviewState::Creating,
        "building" => PreviewState::Building,
        "ready" => PreviewState::Ready,
        "failed" => PreviewState::Failed,
        "destroyed" => PreviewState::Destroyed,
        other => return Err(RepoError::new(format!("unknown preview state: {other}"))),
    };

    Ok(PreviewEnvironment::restore(
        id,
        site_id,
        repo_id,
        pr_number as u32,
        state,
        hostname,
        parse_ts(&created_at)?,
        ready_at.as_deref().map(parse_ts).transpose()?,
        expires_at.as_deref().map(parse_ts).transpose()?,
        destroyed_at.as_deref().map(parse_ts).transpose()?,
        destroy_reason,
    ))
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid timestamp: {e}")))
}

fn map_sqlx(e: sqlx::Error) -> RepoError {
    RepoError::new(e.to_string())
}

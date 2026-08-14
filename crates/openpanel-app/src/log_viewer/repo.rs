//! SQLite adapter for the log viewer bounded context: persisted
//! download audit rows.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    LogDownloadRecord, LogDownloadRepository, LogSource, RepoError,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `LogDownloadRepository`.
#[derive(Clone)]
pub struct SqliteLogViewerRepository {
    pool: SqlitePool,
}

impl SqliteLogViewerRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl LogDownloadRepository for SqliteLogViewerRepository {
    async fn save_download(&self, record: &LogDownloadRecord) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO log_downloads (id, actor, source, service, line_count, downloaded_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(record.id.to_string())
        .bind(record.actor.to_string())
        .bind(record.source.as_str())
        .bind(&record.service)
        .bind(record.line_count as i64)
        .bind(record.downloaded_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_downloads(&self, limit: u32) -> Result<Vec<LogDownloadRecord>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, actor, source, service, line_count, downloaded_at \
             FROM log_downloads ORDER BY downloaded_at DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode).collect()
    }
}

fn decode(row: sqlx::sqlite::SqliteRow) -> Result<LogDownloadRecord, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let actor: String = row.try_get("actor").map_err(map_sqlx)?;
    let source: String = row.try_get("source").map_err(map_sqlx)?;
    let service: String = row.try_get("service").map_err(map_sqlx)?;
    let line_count: i64 = row.try_get("line_count").map_err(map_sqlx)?;
    let downloaded_at: String = row.try_get("downloaded_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let actor = Uuid::parse_str(&actor).map_err(|e| RepoError::new(e.to_string()))?;
    let source = LogSource::from_label(&source)
        .ok_or_else(|| RepoError::new(format!("unknown source: {source}")))?;
    let downloaded_at = parse_ts(&downloaded_at)?;
    Ok(LogDownloadRecord {
        id,
        actor,
        source,
        service,
        line_count: line_count as u32,
        downloaded_at,
    })
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid timestamp: {e}")))
}

fn map_sqlx(e: sqlx::Error) -> RepoError {
    RepoError::new(e.to_string())
}

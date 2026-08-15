//! SQLite adapter for the non-PHP runtime bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{RepoError, RuntimeKind, RuntimeRepository, RuntimeStatus, SiteRuntime};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `RuntimeRepository`.
#[derive(Clone)]
pub struct SqliteRuntimeRepository {
    pool: SqlitePool,
}

impl SqliteRuntimeRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RuntimeRepository for SqliteRuntimeRepository {
    async fn save_runtime(&self, runtime: &SiteRuntime) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO site_runtimes \
             (id, site_id, kind, version, app_port, workdir, start_command, registered_at, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(runtime.id.to_string())
        .bind(runtime.site_id.to_string())
        .bind(runtime.kind.as_str())
        .bind(&runtime.version)
        .bind(runtime.app_port as i64)
        .bind(&runtime.workdir)
        .bind(&runtime.start_command)
        .bind(runtime.registered_at.to_rfc3339())
        .bind(runtime.status.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_runtime(&self, site_id: Uuid) -> Result<Option<SiteRuntime>, RepoError> {
        let row = sqlx::query(
            "SELECT id, site_id, kind, version, app_port, workdir, start_command, registered_at, status \
             FROM site_runtimes WHERE site_id = ?",
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_runtime).transpose()
    }

    async fn list_runtimes(&self) -> Result<Vec<SiteRuntime>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, site_id, kind, version, app_port, workdir, start_command, registered_at, status \
             FROM site_runtimes ORDER BY registered_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_runtime).collect()
    }
}

fn decode_runtime(row: sqlx::sqlite::SqliteRow) -> Result<SiteRuntime, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let kind: String = row.try_get("kind").map_err(map_sqlx)?;
    let version: String = row.try_get("version").map_err(map_sqlx)?;
    let app_port: i64 = row.try_get("app_port").map_err(map_sqlx)?;
    let workdir: String = row.try_get("workdir").map_err(map_sqlx)?;
    let start_command: String = row.try_get("start_command").map_err(map_sqlx)?;
    let registered_at: String = row.try_get("registered_at").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let kind = match kind.as_str() {
        "node" => RuntimeKind::Node,
        "python" => RuntimeKind::Python,
        "go" => RuntimeKind::Go,
        "ruby" => RuntimeKind::Ruby,
        "dotnet" => RuntimeKind::Dotnet,
        other => return Err(RepoError::new(format!("unknown kind: {other}"))),
    };
    let status = match status.as_str() {
        "stopped" => RuntimeStatus::Stopped,
        "running" => RuntimeStatus::Running,
        "failed" => RuntimeStatus::Failed,
        other => return Err(RepoError::new(format!("unknown status: {other}"))),
    };
    let registered_at = parse_ts(&registered_at)?;
    Ok(SiteRuntime {
        id,
        site_id,
        kind,
        version,
        app_port: app_port as u16,
        workdir,
        start_command,
        registered_at,
        status,
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
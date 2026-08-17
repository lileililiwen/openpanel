//! SQLite adapter for the service manager bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{RepoError, ServiceAction, ServiceActionRecord, ServiceManagerRepository};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `ServiceManagerRepository`.
#[derive(Clone)]
pub struct SqliteServiceManagerRepository {
    pool: SqlitePool,
}

impl SqliteServiceManagerRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ServiceManagerRepository for SqliteServiceManagerRepository {
    async fn save_action(&self, record: &ServiceActionRecord) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO service_action_history \
             (id, name, action, actor, recorded_at, success, message) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(record.id.to_string())
        .bind(&record.name)
        .bind(record.action.as_str())
        .bind(record.actor.to_string())
        .bind(record.recorded_at.to_rfc3339())
        .bind(if record.success { 1 } else { 0 })
        .bind(&record.message)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_actions(&self, limit: u32) -> Result<Vec<ServiceActionRecord>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, action, actor, recorded_at, success, message \
             FROM service_action_history ORDER BY recorded_at DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_record).collect()
    }

    async fn list_actions_for(
        &self,
        name: &str,
        limit: u32,
    ) -> Result<Vec<ServiceActionRecord>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, action, actor, recorded_at, success, message \
             FROM service_action_history WHERE name = ? \
             ORDER BY recorded_at DESC LIMIT ?",
        )
        .bind(name)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_record).collect()
    }
}

fn decode_record(row: sqlx::sqlite::SqliteRow) -> Result<ServiceActionRecord, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let name: String = row.try_get("name").map_err(map_sqlx)?;
    let action: String = row.try_get("action").map_err(map_sqlx)?;
    let actor: String = row.try_get("actor").map_err(map_sqlx)?;
    let recorded_at: String = row.try_get("recorded_at").map_err(map_sqlx)?;
    let success: i64 = row.try_get("success").map_err(map_sqlx)?;
    let message: String = row.try_get("message").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let actor = Uuid::parse_str(&actor).map_err(|e| RepoError::new(e.to_string()))?;
    let action = match action.as_str() {
        "start" => ServiceAction::Start,
        "stop" => ServiceAction::Stop,
        "restart" => ServiceAction::Restart,
        "enable" => ServiceAction::Enable,
        "disable" => ServiceAction::Disable,
        other => {
            return Err(RepoError::new(format!("unknown action: {other}")));
        }
    };
    let recorded_at = parse_ts(&recorded_at)?;
    Ok(ServiceActionRecord {
        id,
        name,
        action,
        actor,
        recorded_at,
        success: success != 0,
        message,
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

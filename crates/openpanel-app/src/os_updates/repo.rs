//! SQLite adapter for the OS update management bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    OsUpdateRepository, RebootState, RepoError, UpdateHistoryRecord, UpdateKind, UpdatePolicy,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `OsUpdateRepository`.
#[derive(Clone)]
pub struct SqliteOsUpdateRepository {
    pool: SqlitePool,
}

impl SqliteOsUpdateRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl OsUpdateRepository for SqliteOsUpdateRepository {
    async fn save_policy(&self, policy: &UpdatePolicy) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO update_policy \
             (id, security_auto_install, other_auto_install, run_hour, auto_reboot) \
             VALUES (1, ?, ?, ?, ?)",
        )
        .bind(if policy.security_auto_install { 1 } else { 0 })
        .bind(if policy.other_auto_install { 1 } else { 0 })
        .bind(policy.run_hour as i64)
        .bind(if policy.auto_reboot { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_policy(&self) -> Result<Option<UpdatePolicy>, RepoError> {
        let row = sqlx::query(
            "SELECT security_auto_install, other_auto_install, run_hour, auto_reboot \
             FROM update_policy WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(row.map(decode_policy).transpose()?)
    }

    async fn save_history(&self, record: &UpdateHistoryRecord) -> Result<(), RepoError> {
        let completed_at = record.completed_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO update_history \
             (id, actor, started_at, completed_at, kind, package_count, success, message, \
              reboot_required, kernel_updated) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(record.id.to_string())
        .bind(record.actor.to_string())
        .bind(record.started_at.to_rfc3339())
        .bind(completed_at)
        .bind(record.kind.as_str())
        .bind(record.package_count as i64)
        .bind(if record.success { 1 } else { 0 })
        .bind(&record.message)
        .bind(if record.reboot.required { 1 } else { 0 })
        .bind(if record.reboot.kernel_updated { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_history(&self, limit: u32) -> Result<Vec<UpdateHistoryRecord>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, actor, started_at, completed_at, kind, package_count, success, \
             message, reboot_required, kernel_updated \
             FROM update_history ORDER BY started_at DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_history).collect()
    }
}

fn decode_policy(row: sqlx::sqlite::SqliteRow) -> Result<UpdatePolicy, RepoError> {
    let sec: i64 = row.try_get("security_auto_install").map_err(map_sqlx)?;
    let other: i64 = row.try_get("other_auto_install").map_err(map_sqlx)?;
    let run_hour: i64 = row.try_get("run_hour").map_err(map_sqlx)?;
    let auto_reboot: i64 = row.try_get("auto_reboot").map_err(map_sqlx)?;
    Ok(UpdatePolicy {
        security_auto_install: sec != 0,
        other_auto_install: other != 0,
        run_hour: run_hour as u8,
        auto_reboot: auto_reboot != 0,
    })
}

fn decode_history(row: sqlx::sqlite::SqliteRow) -> Result<UpdateHistoryRecord, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let actor: String = row.try_get("actor").map_err(map_sqlx)?;
    let started_at: String = row.try_get("started_at").map_err(map_sqlx)?;
    let completed_at: Option<String> = row.try_get("completed_at").map_err(map_sqlx)?;
    let kind: String = row.try_get("kind").map_err(map_sqlx)?;
    let package_count: i64 = row.try_get("package_count").map_err(map_sqlx)?;
    let success: i64 = row.try_get("success").map_err(map_sqlx)?;
    let message: String = row.try_get("message").map_err(map_sqlx)?;
    let reboot_required: i64 = row.try_get("reboot_required").map_err(map_sqlx)?;
    let kernel_updated: i64 = row.try_get("kernel_updated").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let actor = Uuid::parse_str(&actor).map_err(|e| RepoError::new(e.to_string()))?;
    let started_at = parse_ts(&started_at)?;
    let completed_at = completed_at.as_deref().map(parse_ts).transpose()?;
    let kind = match kind.as_str() {
        "security" => UpdateKind::Security,
        "other" => UpdateKind::Other,
        other => return Err(RepoError::new(format!("unknown kind: {other}"))),
    };
    Ok(UpdateHistoryRecord {
        id,
        actor,
        started_at,
        completed_at,
        kind,
        package_count: package_count as u32,
        success: success != 0,
        message,
        reboot: RebootState {
            required: reboot_required != 0,
            kernel_updated: kernel_updated != 0,
        },
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

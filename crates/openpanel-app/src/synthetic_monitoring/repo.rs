//! SQLite adapter for the synthetic monitoring bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    CheckResult, CheckStatus, CheckType, RepoError, SyntheticCheck, SyntheticRepository,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `SyntheticRepository`.
#[derive(Clone)]
pub struct SqliteSyntheticRepository {
    pool: SqlitePool,
}

impl SqliteSyntheticRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SyntheticRepository for SqliteSyntheticRepository {
    async fn save_check(&self, check: &SyntheticCheck) -> Result<(), RepoError> {
        let last_run_at = check.last_run_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO synthetic_checks \
             (id, name, kind, target, expected_status, timeout_secs, throttle_secs, \
              warn_before_days, created_at, last_run_at, enabled) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(check.id.to_string())
        .bind(&check.name)
        .bind(check.kind.as_str())
        .bind(&check.target)
        .bind(check.expected_status.map(|v| v as i64))
        .bind(check.timeout_secs as i64)
        .bind(check.throttle_secs as i64)
        .bind(check.warn_before_days as i64)
        .bind(check.created_at.to_rfc3339())
        .bind(last_run_at)
        .bind(if check.enabled { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_check(&self, id: Uuid) -> Result<Option<SyntheticCheck>, RepoError> {
        let row = sqlx::query(
            "SELECT id, name, kind, target, expected_status, timeout_secs, throttle_secs, \
             warn_before_days, created_at, last_run_at, enabled \
             FROM synthetic_checks WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_check).transpose()
    }

    async fn list_checks(&self) -> Result<Vec<SyntheticCheck>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, kind, target, expected_status, timeout_secs, throttle_secs, \
             warn_before_days, created_at, last_run_at, enabled \
             FROM synthetic_checks ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_check).collect()
    }

    async fn touch_check(&self, id: Uuid, ran_at: DateTime<Utc>) -> Result<(), RepoError> {
        sqlx::query("UPDATE synthetic_checks SET last_run_at = ? WHERE id = ?")
            .bind(ran_at.to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete_check(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM synthetic_checks WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_result(&self, result: &CheckResult) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO synthetic_check_runs \
             (id, check_id, ran_at, latency_ms, http_status, cert_days_remaining, status, message) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(result.id.to_string())
        .bind(result.check_id.to_string())
        .bind(result.ran_at.to_rfc3339())
        .bind(result.latency_ms as i64)
        .bind(result.http_status.map(|v| v as i64))
        .bind(result.cert_days_remaining.map(|v| v as i64))
        .bind(result.status.as_str())
        .bind(&result.message)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_results(
        &self,
        check_id: Uuid,
        limit: u32,
    ) -> Result<Vec<CheckResult>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, check_id, ran_at, latency_ms, http_status, cert_days_remaining, status, message \
             FROM synthetic_check_runs WHERE check_id = ? ORDER BY ran_at DESC LIMIT ?",
        )
        .bind(check_id.to_string())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_result).collect()
    }
}

fn decode_check(row: sqlx::sqlite::SqliteRow) -> Result<SyntheticCheck, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let name: String = row.try_get("name").map_err(map_sqlx)?;
    let kind: String = row.try_get("kind").map_err(map_sqlx)?;
    let target: String = row.try_get("target").map_err(map_sqlx)?;
    let expected_status: Option<i64> = row.try_get("expected_status").map_err(map_sqlx)?;
    let timeout_secs: i64 = row.try_get("timeout_secs").map_err(map_sqlx)?;
    let throttle_secs: i64 = row.try_get("throttle_secs").map_err(map_sqlx)?;
    let warn_before_days: i64 = row.try_get("warn_before_days").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let last_run_at: Option<String> = row.try_get("last_run_at").map_err(map_sqlx)?;
    let enabled: i64 = row.try_get("enabled").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let kind = CheckType::from_label(&kind)
        .ok_or_else(|| RepoError::new(format!("unknown check kind: {kind}")))?;
    let created_at = parse_ts(&created_at)?;
    let last_run_at = last_run_at.as_deref().map(parse_ts).transpose()?;
    Ok(SyntheticCheck {
        id,
        name,
        kind,
        target,
        expected_status: expected_status.map(|v| v as u16),
        timeout_secs: timeout_secs as u32,
        throttle_secs: throttle_secs as u32,
        warn_before_days: warn_before_days as u32,
        created_at,
        last_run_at,
        enabled: enabled != 0,
    })
}

fn decode_result(row: sqlx::sqlite::SqliteRow) -> Result<CheckResult, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let check_id: String = row.try_get("check_id").map_err(map_sqlx)?;
    let ran_at: String = row.try_get("ran_at").map_err(map_sqlx)?;
    let latency_ms: i64 = row.try_get("latency_ms").map_err(map_sqlx)?;
    let http_status: Option<i64> = row.try_get("http_status").map_err(map_sqlx)?;
    let cert_days_remaining: Option<i64> = row.try_get("cert_days_remaining").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let message: String = row.try_get("message").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let check_id = Uuid::parse_str(&check_id).map_err(|e| RepoError::new(e.to_string()))?;
    let ran_at = parse_ts(&ran_at)?;
    let status = match status.as_str() {
        "ok" => CheckStatus::Ok,
        "warn" => CheckStatus::Warn,
        "fail" => CheckStatus::Fail,
        other => return Err(RepoError::new(format!("unknown status: {other}"))),
    };
    Ok(CheckResult {
        id,
        check_id,
        ran_at,
        latency_ms: latency_ms as u32,
        http_status: http_status.map(|v| v as u16),
        cert_days_remaining: cert_days_remaining.map(|v| v as u32),
        status,
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

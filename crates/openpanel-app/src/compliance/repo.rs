//! SQLite adapter for the compliance bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    AuditRetentionPolicy, ComplianceRepository, GdprExport, GdprExportPayload, HardeningRun,
    RepoError,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `ComplianceRepository`.
#[derive(Clone)]
pub struct SqliteComplianceRepository {
    pool: SqlitePool,
}

impl SqliteComplianceRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ComplianceRepository for SqliteComplianceRepository {
    async fn save_hardening_run(&self, run: &HardeningRun) -> Result<(), RepoError> {
        let rules_json =
            serde_json::to_string(&run.rules).map_err(|e| RepoError::new(e.to_string()))?;
        let completed_at = run.completed_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO hardening_runs \
             (id, profile, started_at, completed_at, initiated_by, rules_json, has_failures) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id.to_string())
        .bind(&run.profile)
        .bind(run.started_at.to_rfc3339())
        .bind(completed_at)
        .bind(run.initiated_by.to_string())
        .bind(rules_json)
        .bind(if run.has_failures { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_hardening_run(&self, id: Uuid) -> Result<Option<HardeningRun>, RepoError> {
        let row = sqlx::query(
            "SELECT id, profile, started_at, completed_at, initiated_by, rules_json, has_failures \
             FROM hardening_runs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_hardening_run).transpose()
    }

    async fn list_hardening_runs(&self, limit: u32) -> Result<Vec<HardeningRun>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, profile, started_at, completed_at, initiated_by, rules_json, has_failures \
             FROM hardening_runs ORDER BY started_at DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_hardening_run).collect()
    }

    async fn save_retention_policy(&self, policy: &AuditRetentionPolicy) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO audit_retention_policy \
             (id, ttl_days, export_before_purge, updated_at, updated_by) VALUES (1, ?, ?, ?, ?)",
        )
        .bind(policy.ttl_days as i64)
        .bind(if policy.export_before_purge { 1 } else { 0 })
        .bind(policy.updated_at.to_rfc3339())
        .bind(policy.updated_by.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_retention_policy(&self) -> Result<Option<AuditRetentionPolicy>, RepoError> {
        let row = sqlx::query(
            "SELECT ttl_days, export_before_purge, updated_at, updated_by \
             FROM audit_retention_policy WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(row.map(decode_retention_policy).transpose()?)
    }

    async fn save_gdpr_export(&self, export: &GdprExport) -> Result<(), RepoError> {
        let payload =
            serde_json::to_string(&export.payload).map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO gdpr_exports (id, user_id, payload_json, generated_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(export.id.to_string())
        .bind(export.user_id.to_string())
        .bind(payload)
        .bind(export.generated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_gdpr_export(&self, id: Uuid) -> Result<Option<GdprExport>, RepoError> {
        let row = sqlx::query(
            "SELECT id, user_id, payload_json, generated_at FROM gdpr_exports WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_gdpr_export).transpose()
    }

    async fn list_gdpr_exports(&self, user_id: Uuid) -> Result<Vec<GdprExport>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, user_id, payload_json, generated_at FROM gdpr_exports \
             WHERE user_id = ? ORDER BY generated_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_gdpr_export).collect()
    }
}

fn decode_hardening_run(row: sqlx::sqlite::SqliteRow) -> Result<HardeningRun, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let profile: String = row.try_get("profile").map_err(map_sqlx)?;
    let started_at: String = row.try_get("started_at").map_err(map_sqlx)?;
    let completed_at: Option<String> = row.try_get("completed_at").map_err(map_sqlx)?;
    let initiated_by: String = row.try_get("initiated_by").map_err(map_sqlx)?;
    let rules_json: String = row.try_get("rules_json").map_err(map_sqlx)?;
    let has_failures: i64 = row.try_get("has_failures").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let initiated_by = Uuid::parse_str(&initiated_by).map_err(|e| RepoError::new(e.to_string()))?;
    let started_at = parse_ts(&started_at)?;
    let completed_at = completed_at.as_deref().map(parse_ts).transpose()?;
    let rules: Vec<openpanel_domain::HardeningRule> = serde_json::from_str(&rules_json)
        .map_err(|e| RepoError::new(format!("invalid rules_json: {e}")))?;
    Ok(HardeningRun {
        id,
        profile,
        started_at,
        completed_at,
        initiated_by,
        rules,
        has_failures: has_failures != 0,
    })
}

fn decode_retention_policy(
    row: sqlx::sqlite::SqliteRow,
) -> Result<AuditRetentionPolicy, RepoError> {
    let ttl_days: i64 = row.try_get("ttl_days").map_err(map_sqlx)?;
    let export_before_purge: i64 = row.try_get("export_before_purge").map_err(map_sqlx)?;
    let updated_at: String = row.try_get("updated_at").map_err(map_sqlx)?;
    let updated_by: String = row.try_get("updated_by").map_err(map_sqlx)?;
    let updated_at = parse_ts(&updated_at)?;
    let updated_by = Uuid::parse_str(&updated_by)
        .map_err(|e| RepoError::new(format!("invalid updated_by: {e}")))?;
    Ok(AuditRetentionPolicy {
        ttl_days: ttl_days as u32,
        export_before_purge: export_before_purge != 0,
        updated_at,
        updated_by,
    })
}

fn decode_gdpr_export(row: sqlx::sqlite::SqliteRow) -> Result<GdprExport, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let user_id: String = row.try_get("user_id").map_err(map_sqlx)?;
    let payload_json: String = row.try_get("payload_json").map_err(map_sqlx)?;
    let generated_at: String = row.try_get("generated_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let user_id = Uuid::parse_str(&user_id).map_err(|e| RepoError::new(e.to_string()))?;
    let payload: GdprExportPayload = serde_json::from_str(&payload_json)
        .map_err(|e| RepoError::new(format!("invalid payload_json: {e}")))?;
    let generated_at = parse_ts(&generated_at)?;
    Ok(GdprExport {
        id,
        user_id,
        payload,
        generated_at,
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

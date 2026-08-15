//! SQLite adapter for the kernel isolation bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    CgroupLimit, IsolationPolicy, IsolationRepository, RepoError, UserNamespaceConfig,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `IsolationRepository`.
#[derive(Clone)]
pub struct SqliteIsolationRepository {
    pool: SqlitePool,
}

impl SqliteIsolationRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl IsolationRepository for SqliteIsolationRepository {
    async fn save_limit(&self, limit: &CgroupLimit) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO cgroup_limits \
             (user_id, cpu_millicores, memory_high_mib, memory_max_mib, pids_max, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(limit.user_id.to_string())
        .bind(limit.cpu_millicores as i64)
        .bind(limit.memory_high_mib as i64)
        .bind(limit.memory_max_mib as i64)
        .bind(limit.pids_max as i64)
        .bind(limit.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_limit(&self, user_id: Uuid) -> Result<Option<CgroupLimit>, RepoError> {
        let row = sqlx::query(
            "SELECT user_id, cpu_millicores, memory_high_mib, memory_max_mib, pids_max, updated_at \
             FROM cgroup_limits WHERE user_id = ?",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_limit).transpose()
    }

    async fn list_limits(&self) -> Result<Vec<CgroupLimit>, RepoError> {
        let rows = sqlx::query(
            "SELECT user_id, cpu_millicores, memory_high_mib, memory_max_mib, pids_max, updated_at \
             FROM cgroup_limits ORDER BY user_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_limit).collect()
    }

    async fn delete_limit(&self, user_id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM cgroup_limits WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_namespace(&self, config: &UserNamespaceConfig) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO user_namespaces \
             (user_id, enabled, cgroup_namespace, pid_namespace) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(config.user_id.to_string())
        .bind(if config.enabled { 1 } else { 0 })
        .bind(if config.cgroup_namespace { 1 } else { 0 })
        .bind(if config.pid_namespace { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_namespace(
        &self,
        user_id: Uuid,
    ) -> Result<Option<UserNamespaceConfig>, RepoError> {
        let row = sqlx::query(
            "SELECT user_id, enabled, cgroup_namespace, pid_namespace FROM user_namespaces WHERE user_id = ?",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_namespace).transpose()
    }

    async fn save_policy(&self, policy: &IsolationPolicy) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO isolation_policy \
             (id, updated_at, updated_by, default_cpu_millicores, default_memory_high_mib, default_memory_max_mib, default_pids_max) \
             VALUES (1, ?, ?, ?, ?, ?, ?)",
        )
        .bind(policy.updated_at.to_rfc3339())
        .bind(policy.updated_by.to_string())
        .bind(policy.default_cpu_millicores as i64)
        .bind(policy.default_memory_high_mib as i64)
        .bind(policy.default_memory_max_mib as i64)
        .bind(policy.default_pids_max as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_policy(&self) -> Result<Option<IsolationPolicy>, RepoError> {
        let row = sqlx::query(
            "SELECT updated_at, updated_by, default_cpu_millicores, default_memory_high_mib, default_memory_max_mib, default_pids_max \
             FROM isolation_policy WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_policy).transpose()
    }
}

fn decode_limit(row: sqlx::sqlite::SqliteRow) -> Result<CgroupLimit, RepoError> {
    let user_id: String = row.try_get("user_id").map_err(map_sqlx)?;
    let cpu_millicores: i64 = row.try_get("cpu_millicores").map_err(map_sqlx)?;
    let memory_high_mib: i64 = row.try_get("memory_high_mib").map_err(map_sqlx)?;
    let memory_max_mib: i64 = row.try_get("memory_max_mib").map_err(map_sqlx)?;
    let pids_max: i64 = row.try_get("pids_max").map_err(map_sqlx)?;
    let updated_at: String = row.try_get("updated_at").map_err(map_sqlx)?;
    let user_id = Uuid::parse_str(&user_id).map_err(|e| RepoError::new(e.to_string()))?;
    let updated_at = parse_ts(&updated_at)?;
    Ok(CgroupLimit {
        user_id,
        cpu_millicores: cpu_millicores as u32,
        memory_high_mib: memory_high_mib as u32,
        memory_max_mib: memory_max_mib as u32,
        pids_max: pids_max as u32,
        updated_at,
    })
}

fn decode_namespace(row: sqlx::sqlite::SqliteRow) -> Result<UserNamespaceConfig, RepoError> {
    let user_id: String = row.try_get("user_id").map_err(map_sqlx)?;
    let enabled: i64 = row.try_get("enabled").map_err(map_sqlx)?;
    let cgroup_namespace: i64 = row.try_get("cgroup_namespace").map_err(map_sqlx)?;
    let pid_namespace: i64 = row.try_get("pid_namespace").map_err(map_sqlx)?;
    let user_id = Uuid::parse_str(&user_id).map_err(|e| RepoError::new(e.to_string()))?;
    Ok(UserNamespaceConfig {
        user_id,
        enabled: enabled != 0,
        cgroup_namespace: cgroup_namespace != 0,
        pid_namespace: pid_namespace != 0,
    })
}

fn decode_policy(row: sqlx::sqlite::SqliteRow) -> Result<IsolationPolicy, RepoError> {
    let updated_at: String = row.try_get("updated_at").map_err(map_sqlx)?;
    let updated_by: String = row.try_get("updated_by").map_err(map_sqlx)?;
    let default_cpu_millicores: i64 = row.try_get("default_cpu_millicores").map_err(map_sqlx)?;
    let default_memory_high_mib: i64 = row.try_get("default_memory_high_mib").map_err(map_sqlx)?;
    let default_memory_max_mib: i64 = row.try_get("default_memory_max_mib").map_err(map_sqlx)?;
    let default_pids_max: i64 = row.try_get("default_pids_max").map_err(map_sqlx)?;
    let updated_at = parse_ts(&updated_at)?;
    let updated_by = Uuid::parse_str(&updated_by).map_err(|e| RepoError::new(e.to_string()))?;
    Ok(IsolationPolicy {
        updated_at,
        updated_by,
        default_cpu_millicores: default_cpu_millicores as u32,
        default_memory_high_mib: default_memory_high_mib as u32,
        default_memory_max_mib: default_memory_max_mib as u32,
        default_pids_max: default_pids_max as u32,
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
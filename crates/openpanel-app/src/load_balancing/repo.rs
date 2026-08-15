//! SQLite adapter for the load-balancing bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{LbError, LbRepository, LbStatus, Member, Pool, PoolAlgorithm, RepoError};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `LbRepository`.
#[derive(Clone)]
pub struct SqliteLbRepository {
    pool: SqlitePool,
}

impl SqliteLbRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl LbRepository for SqliteLbRepository {
    async fn save_pool(&self, pool: &Pool) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO lb_pools (id, name, algorithm, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(pool.id.to_string())
        .bind(&pool.name)
        .bind(pool.algorithm.as_str())
        .bind(pool.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_pools(&self) -> Result<Vec<Pool>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, algorithm, created_at FROM lb_pools ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_pool).collect()
    }

    async fn get_pool(&self, id: Uuid) -> Result<Option<Pool>, RepoError> {
        let row = sqlx::query(
            "SELECT id, name, algorithm, created_at FROM lb_pools WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_pool).transpose()
    }

    async fn save_member(&self, member: &Member) -> Result<(), RepoError> {
        let last_probe_at = member.last_probe_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO lb_members \
             (id, pool_id, address, weight, status, last_probe_at, failed_probe_count) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(member.id.to_string())
        .bind(member.pool_id.to_string())
        .bind(&member.address)
        .bind(member.weight as i64)
        .bind(member.status.as_str())
        .bind(last_probe_at)
        .bind(member.failed_probe_count as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_members(&self, pool_id: Uuid) -> Result<Vec<Member>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, pool_id, address, weight, status, last_probe_at, failed_probe_count \
             FROM lb_members WHERE pool_id = ?",
        )
        .bind(pool_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_member).collect()
    }

    async fn get_member(&self, id: Uuid) -> Result<Option<Member>, RepoError> {
        let row = sqlx::query(
            "SELECT id, pool_id, address, weight, status, last_probe_at, failed_probe_count \
             FROM lb_members WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_member).transpose()
    }

    async fn delete_member(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM lb_members WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }
}

fn decode_pool(row: sqlx::sqlite::SqliteRow) -> Result<Pool, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let name: String = row.try_get("name").map_err(map_sqlx)?;
    let algorithm: String = row.try_get("algorithm").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let algorithm = match algorithm.as_str() {
        "round_robin" => PoolAlgorithm::RoundRobin,
        "weighted" => PoolAlgorithm::Weighted,
        "sticky_ip" => PoolAlgorithm::StickyIp,
        other => return Err(RepoError::new(format!("unknown algorithm: {other}"))),
    };
    let created_at = parse_ts(&created_at)?;
    Ok(Pool {
        id,
        name,
        algorithm,
        created_at,
    })
}

fn decode_member(row: sqlx::sqlite::SqliteRow) -> Result<Member, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let pool_id: String = row.try_get("pool_id").map_err(map_sqlx)?;
    let address: String = row.try_get("address").map_err(map_sqlx)?;
    let weight: i64 = row.try_get("weight").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let last_probe_at: Option<String> = row.try_get("last_probe_at").map_err(map_sqlx)?;
    let failed_probe_count: i64 = row.try_get("failed_probe_count").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let pool_id = Uuid::parse_str(&pool_id).map_err(|e| RepoError::new(e.to_string()))?;
    let status = match status.as_str() {
        "healthy" => LbStatus::Healthy,
        "failing" => LbStatus::Failing,
        "disabled" => LbStatus::Disabled,
        other => return Err(RepoError::new(format!("unknown status: {other}"))),
    };
    let last_probe_at = last_probe_at.as_deref().map(parse_ts).transpose()?;
    Ok(Member {
        id,
        pool_id,
        address,
        weight: weight as u32,
        status,
        last_probe_at,
        failed_probe_count: failed_probe_count as u32,
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

#[allow(dead_code)]
fn _lb_error_translation(e: LbError) -> RepoError {
    RepoError::new(e.to_string())
}

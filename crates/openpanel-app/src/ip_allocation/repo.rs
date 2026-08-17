//! SQLite adapter for the IPv6 + address-pool bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    IpAllocation, IpFamily, IpPool, IpRepository, IpStatus, PoolKind, RepoError,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `IpRepository`.
#[derive(Clone)]
pub struct SqliteIpRepository {
    pool: SqlitePool,
}

impl SqliteIpRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl IpRepository for SqliteIpRepository {
    async fn save_pool(&self, pool: &IpPool) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO ip_pools (id, name, kind, family, cidr, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(pool.id.to_string())
        .bind(&pool.name)
        .bind(pool.kind.as_str())
        .bind(pool.family.as_str())
        .bind(&pool.cidr)
        .bind(pool.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_pools(&self) -> Result<Vec<IpPool>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, kind, family, cidr, created_at FROM ip_pools ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_pool).collect()
    }

    async fn get_pool(&self, id: Uuid) -> Result<Option<IpPool>, RepoError> {
        let row = sqlx::query(
            "SELECT id, name, kind, family, cidr, created_at FROM ip_pools WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_pool).transpose()
    }

    async fn delete_pool(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM ip_pools WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_allocation(&self, allocation: &IpAllocation) -> Result<(), RepoError> {
        let bound_at = allocation.bound_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO ip_allocations \
             (id, pool_id, address, site_id, status, allocated_at, bound_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(allocation.id.to_string())
        .bind(allocation.pool_id.to_string())
        .bind(&allocation.address)
        .bind(allocation.site_id.to_string())
        .bind(allocation.status.as_str())
        .bind(allocation.allocated_at.to_rfc3339())
        .bind(bound_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_allocations_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<IpAllocation>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, pool_id, address, site_id, status, allocated_at, bound_at \
             FROM ip_allocations WHERE site_id = ?",
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_allocation).collect()
    }

    async fn list_allocations_for_pool(
        &self,
        pool_id: Uuid,
    ) -> Result<Vec<IpAllocation>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, pool_id, address, site_id, status, allocated_at, bound_at \
             FROM ip_allocations WHERE pool_id = ?",
        )
        .bind(pool_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_allocation).collect()
    }

    async fn get_allocation_by_address(
        &self,
        pool_id: Uuid,
        address: &str,
    ) -> Result<Option<IpAllocation>, RepoError> {
        let row = sqlx::query(
            "SELECT id, pool_id, address, site_id, status, allocated_at, bound_at \
             FROM ip_allocations WHERE pool_id = ? AND address = ?",
        )
        .bind(pool_id.to_string())
        .bind(address)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_allocation).transpose()
    }
}

fn decode_pool(row: sqlx::sqlite::SqliteRow) -> Result<IpPool, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let name: String = row.try_get("name").map_err(map_sqlx)?;
    let kind: String = row.try_get("kind").map_err(map_sqlx)?;
    let family: String = row.try_get("family").map_err(map_sqlx)?;
    let cidr: String = row.try_get("cidr").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let kind = match kind.as_str() {
        "shared" => PoolKind::Shared,
        "dedicated" => PoolKind::Dedicated,
        other => return Err(RepoError::new(format!("unknown pool kind: {other}"))),
    };
    let family = match family.as_str() {
        "v4" => IpFamily::V4,
        "v6" => IpFamily::V6,
        other => return Err(RepoError::new(format!("unknown family: {other}"))),
    };
    let created_at = parse_ts(&created_at)?;
    Ok(IpPool {
        id,
        name,
        kind,
        family,
        cidr,
        created_at,
    })
}

fn decode_allocation(row: sqlx::sqlite::SqliteRow) -> Result<IpAllocation, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let pool_id: String = row.try_get("pool_id").map_err(map_sqlx)?;
    let address: String = row.try_get("address").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let allocated_at: String = row.try_get("allocated_at").map_err(map_sqlx)?;
    let bound_at: Option<String> = row.try_get("bound_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let pool_id = Uuid::parse_str(&pool_id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let status = match status.as_str() {
        "reserved" => IpStatus::Reserved,
        "active" => IpStatus::Active,
        "released" => IpStatus::Released,
        other => return Err(RepoError::new(format!("unknown status: {other}"))),
    };
    let allocated_at = parse_ts(&allocated_at)?;
    let bound_at = bound_at.as_deref().map(parse_ts).transpose()?;
    Ok(IpAllocation {
        id,
        pool_id,
        address,
        site_id,
        status,
        allocated_at,
        bound_at,
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

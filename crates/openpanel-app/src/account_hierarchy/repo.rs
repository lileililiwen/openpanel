//! SQLite-backed adapter for the account-hierarchy bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    AccountHierarchyError, AccountRelationship, HierarchyNode, HierarchyRepository,
    HierarchyStatus, PoolAxis, PoolClaim, QuotaPool, RepoError,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed repository for the hierarchy bounded context.
#[derive(Clone)]
pub struct SqliteHierarchyRepository {
    pool: Pool<Sqlite>,
}

impl SqliteHierarchyRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HierarchyRepository for SqliteHierarchyRepository {
    async fn insert_relationship(
        &self,
        relationship: &AccountRelationship,
    ) -> Result<(), AccountHierarchyError> {
        sqlx::query(
            "INSERT INTO account_relationships (parent_id, child_id, created_at, created_by, status) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(relationship.parent_id().to_string())
        .bind(relationship.child_id().to_string())
        .bind(relationship.created_at().to_rfc3339())
        .bind(relationship.created_by().to_string())
        .bind(relationship_status_str(relationship.status()))
        .execute(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn children_of(
        &self,
        parent_id: Uuid,
    ) -> Result<Vec<AccountRelationship>, AccountHierarchyError> {
        let rows: Vec<RelationshipRow> = sqlx::query_as::<_, RelationshipRow>(
            "SELECT parent_id, child_id, created_at, created_by, status FROM account_relationships WHERE parent_id = ? AND status = 'active' ORDER BY created_at",
        )
        .bind(parent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        rows.into_iter()
            .map(RelationshipRow::into_relationship)
            .collect()
    }

    async fn parent_of(
        &self,
        child_id: Uuid,
    ) -> Result<Option<AccountRelationship>, AccountHierarchyError> {
        let row: Option<RelationshipRow> = sqlx::query_as::<_, RelationshipRow>(
            "SELECT parent_id, child_id, created_at, created_by, status FROM account_relationships WHERE child_id = ? AND status = 'active' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(child_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        row.map(RelationshipRow::into_relationship).transpose()
    }

    async fn detach(&self, parent_id: Uuid, child_id: Uuid) -> Result<(), AccountHierarchyError> {
        sqlx::query("UPDATE account_relationships SET status = 'detached' WHERE parent_id = ? AND child_id = ?")
            .bind(parent_id.to_string())
            .bind(child_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn upsert_pool(&self, pool: &QuotaPool) -> Result<(), AccountHierarchyError> {
        sqlx::query(
            "INSERT INTO quota_pools (parent_id, axis, total_bytes, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT(parent_id, axis) DO UPDATE SET total_bytes = excluded.total_bytes, updated_at = excluded.updated_at",
        )
        .bind(pool.parent_id().to_string())
        .bind(pool_axis_str(pool.axis()))
        .bind(pool.total_bytes() as i64)
        .bind(pool.updated_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn pools_of(&self, parent_id: Uuid) -> Result<Vec<QuotaPool>, AccountHierarchyError> {
        let rows: Vec<PoolRow> = sqlx::query_as::<_, PoolRow>(
            "SELECT parent_id, axis, total_bytes, updated_at FROM quota_pools WHERE parent_id = ? ORDER BY axis",
        )
        .bind(parent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        rows.into_iter().map(PoolRow::into_pool).collect()
    }

    async fn insert_claim(&self, claim: &PoolClaim) -> Result<(), AccountHierarchyError> {
        sqlx::query(
            "INSERT INTO pool_claims (parent_id, child_id, axis, share_bytes, claimed_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(claim.parent_id().to_string())
        .bind(claim.child_id().to_string())
        .bind(pool_axis_str(claim.axis()))
        .bind(claim.share_bytes() as i64)
        .bind(claim.claimed_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn claims_of(&self, parent_id: Uuid) -> Result<Vec<PoolClaim>, AccountHierarchyError> {
        let rows: Vec<ClaimRow> = sqlx::query_as::<_, ClaimRow>(
            "SELECT parent_id, child_id, axis, share_bytes, claimed_at FROM pool_claims WHERE parent_id = ? ORDER BY axis, claimed_at",
        )
        .bind(parent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        rows.into_iter().map(ClaimRow::into_claim).collect()
    }

    async fn remove_claim(
        &self,
        parent_id: Uuid,
        child_id: Uuid,
        axis: PoolAxis,
    ) -> Result<(), AccountHierarchyError> {
        sqlx::query("DELETE FROM pool_claims WHERE parent_id = ? AND child_id = ? AND axis = ?")
            .bind(parent_id.to_string())
            .bind(child_id.to_string())
            .bind(pool_axis_str(axis))
            .execute(&self.pool)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn pool_usage(
        &self,
        parent_id: Uuid,
        axis: PoolAxis,
    ) -> Result<u64, AccountHierarchyError> {
        let total: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(share_bytes), 0) FROM pool_claims WHERE parent_id = ? AND axis = ?",
        )
        .bind(parent_id.to_string())
        .bind(pool_axis_str(axis))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        Ok(total.unwrap_or(0) as u64)
    }

    async fn path_exists(
        &self,
        ancestor: Uuid,
        descendant: Uuid,
    ) -> Result<bool, AccountHierarchyError> {
        // Walk from descendant to its parents; if any of them is
        // `ancestor`, a cycle exists.
        let mut current = descendant;
        let mut guard = 0_u32;
        loop {
            if current == ancestor {
                return Ok(true);
            }
            if guard > 1024 {
                return Ok(false);
            }
            let next: Option<(String,)> = sqlx::query_as(
                "SELECT parent_id FROM account_relationships WHERE child_id = ? AND status = 'active' LIMIT 1",
            )
            .bind(current.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
            match next {
                Some((parent,)) => {
                    let parent_id = Uuid::parse_str(&parent)
                        .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
                    current = parent_id;
                }
                None => return Ok(false),
            }
            guard += 1;
        }
    }

    async fn exists(&self, id: Uuid) -> Result<bool, RepoError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(n > 0)
    }
}

fn relationship_status_str(status: HierarchyStatus) -> &'static str {
    match status {
        HierarchyStatus::Active => "active",
        HierarchyStatus::Detached => "detached",
    }
}

fn parse_relationship_status(s: &str) -> Result<HierarchyStatus, AccountHierarchyError> {
    match s {
        "active" => Ok(HierarchyStatus::Active),
        "detached" => Ok(HierarchyStatus::Detached),
        other => Err(AccountHierarchyError::Persistence(format!(
            "unknown hierarchy status `{other}`"
        ))),
    }
}

fn pool_axis_str(axis: PoolAxis) -> &'static str {
    match axis {
        PoolAxis::DiskBytes => "disk_bytes",
        PoolAxis::BandwidthBytesPerMonth => "bandwidth_bytes_per_month",
        PoolAxis::MaxChildAccounts => "max_child_accounts",
    }
}

fn parse_pool_axis(s: &str) -> Result<PoolAxis, AccountHierarchyError> {
    match s {
        "disk_bytes" => Ok(PoolAxis::DiskBytes),
        "bandwidth_bytes_per_month" => Ok(PoolAxis::BandwidthBytesPerMonth),
        "max_child_accounts" => Ok(PoolAxis::MaxChildAccounts),
        other => Err(AccountHierarchyError::Persistence(format!(
            "unknown pool axis `{other}`"
        ))),
    }
}

#[derive(sqlx::FromRow)]
struct RelationshipRow {
    parent_id: String,
    child_id: String,
    created_at: String,
    created_by: String,
    status: String,
}

impl RelationshipRow {
    fn into_relationship(self) -> Result<AccountRelationship, AccountHierarchyError> {
        let parent_id = Uuid::parse_str(&self.parent_id)
            .map_err(|e| AccountHierarchyError::Persistence(format!("bad parent_id: {e}")))?;
        let child_id = Uuid::parse_str(&self.child_id)
            .map_err(|e| AccountHierarchyError::Persistence(format!("bad child_id: {e}")))?;
        let created_by = Uuid::parse_str(&self.created_by)
            .map_err(|e| AccountHierarchyError::Persistence(format!("bad created_by: {e}")))?;
        let created_at = parse_dt(&self.created_at)?;
        let _status = parse_relationship_status(&self.status)?;
        Ok(AccountRelationship::new(
            parent_id, child_id, created_by, created_at,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct PoolRow {
    parent_id: String,
    axis: String,
    total_bytes: i64,
    updated_at: String,
}

impl PoolRow {
    fn into_pool(self) -> Result<QuotaPool, AccountHierarchyError> {
        let parent_id = Uuid::parse_str(&self.parent_id)
            .map_err(|e| AccountHierarchyError::Persistence(format!("bad parent_id: {e}")))?;
        let axis = parse_pool_axis(&self.axis)?;
        let updated_at = parse_dt(&self.updated_at)?;
        QuotaPool::new(parent_id, axis, self.total_bytes as u64, updated_at)
    }
}

#[derive(sqlx::FromRow)]
struct ClaimRow {
    parent_id: String,
    child_id: String,
    axis: String,
    share_bytes: i64,
    claimed_at: String,
}

impl ClaimRow {
    fn into_claim(self) -> Result<PoolClaim, AccountHierarchyError> {
        let parent_id = Uuid::parse_str(&self.parent_id)
            .map_err(|e| AccountHierarchyError::Persistence(format!("bad parent_id: {e}")))?;
        let child_id = Uuid::parse_str(&self.child_id)
            .map_err(|e| AccountHierarchyError::Persistence(format!("bad child_id: {e}")))?;
        let axis = parse_pool_axis(&self.axis)?;
        let claimed_at = parse_dt(&self.claimed_at)?;
        PoolClaim::new(
            parent_id,
            child_id,
            axis,
            self.share_bytes as u64,
            claimed_at,
        )
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, AccountHierarchyError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AccountHierarchyError::Persistence(format!("bad timestamp `{s}`: {e}")))
}

#[allow(dead_code)]
fn _force_node(r: &HierarchyNode) {
    let _ = r;
}

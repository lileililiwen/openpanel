//! SQLite-backed adapters for the `site-staging` repository traits.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    PromotionRepository, PromotionRun, PromotionStatus, SiteStagingError, SnapshotId, StagingSlot,
    StagingSlotRepository, SyncPolicy,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

fn parse_status(s: &str) -> Result<PromotionStatus, SiteStagingError> {
    match s {
        "pending" => Ok(PromotionStatus::Pending),
        "promoting" => Ok(PromotionStatus::Promoting),
        "promoted" => Ok(PromotionStatus::Promoted),
        "rolled_back" => Ok(PromotionStatus::RolledBack),
        "failed" => Ok(PromotionStatus::Failed),
        other => Err(SiteStagingError::Invalid(format!(
            "unknown promotion status `{other}`"
        ))),
    }
}

fn parse_policy(s: &str) -> Result<SyncPolicy, SiteStagingError> {
    match s {
        "on_demand" => Ok(SyncPolicy::OnDemand),
        "on_promote" => Ok(SyncPolicy::OnPromote),
        "scheduled" => Ok(SyncPolicy::Scheduled),
        other => Err(SiteStagingError::Invalid(format!(
            "unknown sync policy `{other}`"
        ))),
    }
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, SiteStagingError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| SiteStagingError::Invalid(format!("invalid timestamp `{s}`: {e}")))
}

fn parse_optional_ts(s: Option<String>) -> Result<Option<DateTime<Utc>>, SiteStagingError> {
    match s {
        Some(value) => parse_ts(&value).map(Some),
        None => Ok(None),
    }
}

fn parse_optional_snapshot(s: Option<i64>) -> Result<Option<SnapshotId>, SiteStagingError> {
    match s {
        Some(value) => SnapshotId::new(value).map(Some),
        None => Ok(None),
    }
}

/// SQLite-backed implementation of `StagingSlotRepository`.
#[derive(Clone)]
pub struct SqliteStagingSlotRepository {
    pool: Pool<Sqlite>,
}

impl SqliteStagingSlotRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StagingSlotRepository for SqliteStagingSlotRepository {
    async fn insert(&self, slot: &StagingSlot) -> Result<(), SiteStagingError> {
        sqlx::query(
            r#"INSERT INTO staging_slots
                (id, site_id, subdomain, document_root, db_name, php_version,
                 sync_policy, schedule, current_snapshot, last_promoted_snapshot,
                 created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(slot.id().to_string())
        .bind(slot.site_id().to_string())
        .bind(slot.subdomain())
        .bind(slot.document_root())
        .bind(slot.db_name())
        .bind(slot.php_version())
        .bind(slot.sync_policy().as_str())
        .bind(slot.schedule())
        .bind(slot.current_snapshot().map(|s| s.as_i64()))
        .bind(slot.last_promoted_snapshot().map(|s| s.as_i64()))
        .bind(slot.created_at().to_rfc3339())
        .bind(slot.updated_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<StagingSlot>, SiteStagingError> {
        let row: Option<SlotRow> = sqlx::query_as::<_, SlotRow>(
            r#"SELECT id, site_id, subdomain, document_root, db_name, php_version,
                      sync_policy, schedule, current_snapshot, last_promoted_snapshot,
                      created_at, updated_at
                 FROM staging_slots WHERE id = ?"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        row.map(SlotRow::into_slot).transpose()
    }

    async fn find_by_site(&self, site_id: Uuid) -> Result<Option<StagingSlot>, SiteStagingError> {
        let row: Option<SlotRow> = sqlx::query_as::<_, SlotRow>(
            r#"SELECT id, site_id, subdomain, document_root, db_name, php_version,
                      sync_policy, schedule, current_snapshot, last_promoted_snapshot,
                      created_at, updated_at
                 FROM staging_slots WHERE site_id = ?"#,
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        row.map(SlotRow::into_slot).transpose()
    }

    async fn list_all(&self) -> Result<Vec<StagingSlot>, SiteStagingError> {
        let rows: Vec<SlotRow> = sqlx::query_as::<_, SlotRow>(
            r#"SELECT id, site_id, subdomain, document_root, db_name, php_version,
                      sync_policy, schedule, current_snapshot, last_promoted_snapshot,
                      created_at, updated_at
                 FROM staging_slots ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        rows.into_iter().map(SlotRow::into_slot).collect()
    }

    async fn update(&self, slot: &StagingSlot) -> Result<(), SiteStagingError> {
        sqlx::query(
            r#"UPDATE staging_slots
                  SET subdomain = ?, document_root = ?, db_name = ?,
                      php_version = ?, sync_policy = ?, schedule = ?,
                      current_snapshot = ?, last_promoted_snapshot = ?,
                      updated_at = ?
                  WHERE id = ?"#,
        )
        .bind(slot.subdomain())
        .bind(slot.document_root())
        .bind(slot.db_name())
        .bind(slot.php_version())
        .bind(slot.sync_policy().as_str())
        .bind(slot.schedule())
        .bind(slot.current_snapshot().map(|s| s.as_i64()))
        .bind(slot.last_promoted_snapshot().map(|s| s.as_i64()))
        .bind(slot.updated_at().to_rfc3339())
        .bind(slot.id().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), SiteStagingError> {
        sqlx::query("DELETE FROM staging_slots WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SlotRow {
    id: String,
    site_id: String,
    subdomain: String,
    document_root: String,
    db_name: String,
    php_version: Option<String>,
    sync_policy: String,
    schedule: Option<String>,
    current_snapshot: Option<i64>,
    last_promoted_snapshot: Option<i64>,
    created_at: String,
    updated_at: String,
}

impl SlotRow {
    fn into_slot(self) -> Result<StagingSlot, SiteStagingError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| SiteStagingError::Invalid(format!("invalid id: {e}")))?;
        let site_id = Uuid::parse_str(&self.site_id)
            .map_err(|e| SiteStagingError::Invalid(format!("invalid site_id: {e}")))?;
        let sync_policy = parse_policy(&self.sync_policy)?;
        let current_snapshot = parse_optional_snapshot(self.current_snapshot)?;
        let last_promoted_snapshot = parse_optional_snapshot(self.last_promoted_snapshot)?;
        let created_at = parse_ts(&self.created_at)?;
        let updated_at = parse_ts(&self.updated_at)?;
        Ok(StagingSlot::restore(
            id,
            site_id,
            self.subdomain,
            self.document_root,
            self.db_name,
            self.php_version,
            sync_policy,
            self.schedule,
            current_snapshot,
            last_promoted_snapshot,
            created_at,
            updated_at,
        ))
    }
}

/// SQLite-backed implementation of `SnapshotRepository`.
#[derive(Clone)]
pub struct SqliteStagingSnapshotRepository {
    pool: Pool<Sqlite>,
}

impl SqliteStagingSnapshotRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl openpanel_domain::StagingSnapshotRepository for SqliteStagingSnapshotRepository {
    async fn insert(
        &self,
        site_id: Uuid,
        snapshot: SnapshotId,
        taken_at: DateTime<Utc>,
    ) -> Result<(), SiteStagingError> {
        sqlx::query(
            "INSERT INTO staging_snapshots (site_id, snapshot, taken_at)
             VALUES (?, ?, ?)",
        )
        .bind(site_id.to_string())
        .bind(snapshot.as_i64())
        .bind(taken_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_by_site(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<openpanel_domain::site_staging::SnapshotRow>, SiteStagingError> {
        let rows: Vec<SnapshotRow> = sqlx::query_as::<_, SnapshotRow>(
            "SELECT site_id, snapshot, taken_at FROM staging_snapshots
                 WHERE site_id = ? ORDER BY snapshot DESC",
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        rows.into_iter()
            .map(|r| {
                let site_id = Uuid::parse_str(&r.site_id)
                    .map_err(|e| SiteStagingError::Invalid(format!("invalid site_id: {e}")))?;
                let snapshot = SnapshotId::new(r.snapshot)?;
                let taken_at = parse_ts(&r.taken_at)?;
                Ok(openpanel_domain::site_staging::SnapshotRow {
                    site_id,
                    snapshot,
                    taken_at,
                })
            })
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    site_id: String,
    snapshot: i64,
    taken_at: String,
}

/// SQLite-backed implementation of `PromotionRepository`.
#[derive(Clone)]
pub struct SqlitePromotionRepository {
    pool: Pool<Sqlite>,
}

impl SqlitePromotionRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PromotionRepository for SqlitePromotionRepository {
    async fn insert(&self, run: &PromotionRun) -> Result<(), SiteStagingError> {
        sqlx::query(
            r#"INSERT INTO promotion_runs
                (id, site_id, snapshot, confirmed_at, status, requested_by,
                 requested_at, updated_at, failure_reason)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(run.id().to_string())
        .bind(run.site_id().to_string())
        .bind(run.snapshot().as_i64())
        .bind(run.confirmed_at().to_rfc3339())
        .bind(run.status().as_str())
        .bind(run.requested_by())
        .bind(run.requested_at().to_rfc3339())
        .bind(run.updated_at().to_rfc3339())
        .bind(run.failure_reason().map(str::to_owned))
        .execute(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PromotionRun>, SiteStagingError> {
        let row: Option<PromotionRow> = sqlx::query_as::<_, PromotionRow>(
            r#"SELECT id, site_id, snapshot, confirmed_at, status, requested_by,
                      requested_at, updated_at, failure_reason
                 FROM promotion_runs WHERE id = ?"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        row.map(PromotionRow::into_run).transpose()
    }

    async fn list_by_site(&self, site_id: Uuid) -> Result<Vec<PromotionRun>, SiteStagingError> {
        let rows: Vec<PromotionRow> = sqlx::query_as::<_, PromotionRow>(
            r#"SELECT id, site_id, snapshot, confirmed_at, status, requested_by,
                      requested_at, updated_at, failure_reason
                 FROM promotion_runs WHERE site_id = ?
                 ORDER BY requested_at DESC"#,
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        rows.into_iter().map(PromotionRow::into_run).collect()
    }

    async fn update(&self, run: &PromotionRun) -> Result<(), SiteStagingError> {
        sqlx::query(
            r#"UPDATE promotion_runs
                  SET snapshot = ?, confirmed_at = ?, status = ?,
                      requested_by = ?, requested_at = ?, updated_at = ?,
                      failure_reason = ?
                  WHERE id = ?"#,
        )
        .bind(run.snapshot().as_i64())
        .bind(run.confirmed_at().to_rfc3339())
        .bind(run.status().as_str())
        .bind(run.requested_by())
        .bind(run.requested_at().to_rfc3339())
        .bind(run.updated_at().to_rfc3339())
        .bind(run.failure_reason().map(str::to_owned))
        .bind(run.id().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct PromotionRow {
    id: String,
    site_id: String,
    snapshot: i64,
    confirmed_at: String,
    status: String,
    requested_by: String,
    requested_at: String,
    updated_at: String,
    failure_reason: Option<String>,
}

impl PromotionRow {
    fn into_run(self) -> Result<PromotionRun, SiteStagingError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| SiteStagingError::Invalid(format!("invalid id: {e}")))?;
        let site_id = Uuid::parse_str(&self.site_id)
            .map_err(|e| SiteStagingError::Invalid(format!("invalid site_id: {e}")))?;
        let snapshot = SnapshotId::new(self.snapshot)?;
        let confirmed_at = parse_ts(&self.confirmed_at)?;
        let status = parse_status(&self.status)?;
        let requested_at = parse_ts(&self.requested_at)?;
        let updated_at = parse_ts(&self.updated_at)?;
        let _ = parse_optional_ts(Some(self.requested_at.clone()))?;
        Ok(PromotionRun::restore(
            id,
            site_id,
            snapshot,
            confirmed_at,
            status,
            self.requested_by,
            requested_at,
            updated_at,
            self.failure_reason,
        ))
    }
}

/// In-process lock table for staging slots.
#[derive(Clone)]
pub struct SqliteStagingLockTable {
    pool: Pool<Sqlite>,
}

impl SqliteStagingLockTable {
    /// Build a new lock table over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Try to acquire a lock. Returns `true` on success.
    pub async fn try_acquire(
        &self,
        site_id: Uuid,
        locked_by: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, SiteStagingError> {
        let res = sqlx::query(
            "INSERT INTO staging_locks (site_id, locked_by, locked_at)
             VALUES (?, ?, ?)
             ON CONFLICT(site_id) DO UPDATE
             SET locked_by = excluded.locked_by,
                   locked_at = excluded.locked_at
             WHERE datetime(staging_locks.locked_at, '+300 seconds') < datetime(?, '+0 seconds')
                OR staging_locks.locked_by = excluded.locked_by",
        )
        .bind(site_id.to_string())
        .bind(locked_by)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(res.rows_affected() == 1)
    }

    /// Release the lock for `site_id` if held by `locked_by`.
    pub async fn release(&self, site_id: Uuid, locked_by: &str) -> Result<(), SiteStagingError> {
        sqlx::query("DELETE FROM staging_locks WHERE site_id = ? AND locked_by = ?")
            .bind(site_id.to_string())
            .bind(locked_by)
            .execute(&self.pool)
            .await
            .map_err(|e| SiteStagingError::Database(e.to_string()))?;
        Ok(())
    }
}

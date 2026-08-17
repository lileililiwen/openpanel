//! SQLite-backed adapter for the quotas bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    QuotaError, QuotaLimit, QuotaPolicy, QuotaRepository, QuotaSubject, QuotaUsage,
    quotas::QuotaDimension,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed quotas repository.
#[derive(Clone)]
pub struct SqliteQuotaRepository {
    pool: Pool<Sqlite>,
}

impl SqliteQuotaRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QuotaRepository for SqliteQuotaRepository {
    async fn insert(&self, policy: &QuotaPolicy) -> Result<(), QuotaError> {
        sqlx::query(
            "INSERT INTO quota_policies (id, subject_kind, subject_id, disk_soft_bytes, disk_hard_bytes, disk_grace_days, bandwidth_soft_bytes, bandwidth_hard_bytes, bandwidth_grace_days, inodes_soft, inodes_hard, inodes_grace_days, max_file_size_bytes, cpu_shares, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(policy.id().to_string())
        .bind(subject_kind_str(policy.subject_kind()))
        .bind(policy.subject_id().to_string())
        .bind(policy.disk().soft_bytes as i64)
        .bind(policy.disk().hard_bytes as i64)
        .bind(policy.disk().grace_days as i64)
        .bind(policy.bandwidth().soft_bytes as i64)
        .bind(policy.bandwidth().hard_bytes as i64)
        .bind(policy.bandwidth().grace_days as i64)
        .bind(policy.inodes().soft_bytes as i64)
        .bind(policy.inodes().hard_bytes as i64)
        .bind(policy.inodes().grace_days as i64)
        .bind(policy.max_file_size_bytes().map(|v| v as i64))
        .bind(policy.cpu_shares().map(|v| v as i64))
        .bind(policy.created_at().to_rfc3339())
        .bind(policy.updated_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<QuotaPolicy>, QuotaError> {
        let row: Option<PolicyRow> = sqlx::query_as::<_, PolicyRow>(
            "SELECT id, subject_kind, subject_id, disk_soft_bytes, disk_hard_bytes, disk_grace_days, bandwidth_soft_bytes, bandwidth_hard_bytes, bandwidth_grace_days, inodes_soft, inodes_hard, inodes_grace_days, max_file_size_bytes, cpu_shares, created_at, updated_at FROM quota_policies WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        row.map(PolicyRow::into_policy).transpose()
    }

    async fn find_by_subject(
        &self,
        subject_kind: QuotaSubject,
        subject_id: Uuid,
    ) -> Result<Option<QuotaPolicy>, QuotaError> {
        let row: Option<PolicyRow> = sqlx::query_as::<_, PolicyRow>(
            "SELECT id, subject_kind, subject_id, disk_soft_bytes, disk_hard_bytes, disk_grace_days, bandwidth_soft_bytes, bandwidth_hard_bytes, bandwidth_grace_days, inodes_soft, inodes_hard, inodes_grace_days, max_file_size_bytes, cpu_shares, created_at, updated_at FROM quota_policies WHERE subject_kind = ? AND subject_id = ?",
        )
        .bind(subject_kind_str(subject_kind))
        .bind(subject_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        row.map(PolicyRow::into_policy).transpose()
    }

    async fn update(&self, policy: &QuotaPolicy) -> Result<(), QuotaError> {
        sqlx::query(
            "UPDATE quota_policies SET disk_soft_bytes = ?, disk_hard_bytes = ?, disk_grace_days = ?, bandwidth_soft_bytes = ?, bandwidth_hard_bytes = ?, bandwidth_grace_days = ?, inodes_soft = ?, inodes_hard = ?, inodes_grace_days = ?, max_file_size_bytes = ?, cpu_shares = ?, updated_at = ? WHERE id = ?",
        )
        .bind(policy.disk().soft_bytes as i64)
        .bind(policy.disk().hard_bytes as i64)
        .bind(policy.disk().grace_days as i64)
        .bind(policy.bandwidth().soft_bytes as i64)
        .bind(policy.bandwidth().hard_bytes as i64)
        .bind(policy.bandwidth().grace_days as i64)
        .bind(policy.inodes().soft_bytes as i64)
        .bind(policy.inodes().hard_bytes as i64)
        .bind(policy.inodes().grace_days as i64)
        .bind(policy.max_file_size_bytes().map(|v| v as i64))
        .bind(policy.cpu_shares().map(|v| v as i64))
        .bind(policy.updated_at().to_rfc3339())
        .bind(policy.id().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), QuotaError> {
        sqlx::query("DELETE FROM quota_policies WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<QuotaPolicy>, QuotaError> {
        let rows: Vec<PolicyRow> = sqlx::query_as::<_, PolicyRow>(
            "SELECT id, subject_kind, subject_id, disk_soft_bytes, disk_hard_bytes, disk_grace_days, bandwidth_soft_bytes, bandwidth_hard_bytes, bandwidth_grace_days, inodes_soft, inodes_hard, inodes_grace_days, max_file_size_bytes, cpu_shares, created_at, updated_at FROM quota_policies ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        rows.into_iter().map(PolicyRow::into_policy).collect()
    }

    async fn insert_usage(&self, usage: &QuotaUsage) -> Result<(), QuotaError> {
        let over_soft: Vec<String> = usage.over_soft.iter().map(|d| dimension_str(*d)).collect();
        let over_hard: Vec<String> = usage.over_hard.iter().map(|d| dimension_str(*d)).collect();
        sqlx::query(
            "INSERT INTO quota_usages (subject_id, sampled_at, disk_used_bytes, disk_inodes_used, bandwidth_used_bytes, over_soft_json, over_hard_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(usage.subject_id.to_string())
        .bind(usage.sampled_at.to_rfc3339())
        .bind(usage.disk_used_bytes as i64)
        .bind(usage.disk_inodes_used as i64)
        .bind(usage.bandwidth_used_bytes_this_month as i64)
        .bind(serde_json::to_string(&over_soft).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&over_hard).unwrap_or_else(|_| "[]".to_string()))
        .execute(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn latest_usage(&self, subject_id: Uuid) -> Result<Option<QuotaUsage>, QuotaError> {
        let row: Option<UsageRow> = sqlx::query_as::<_, UsageRow>(
            "SELECT subject_id, sampled_at, disk_used_bytes, disk_inodes_used, bandwidth_used_bytes, over_soft_json, over_hard_json FROM quota_usages WHERE subject_id = ? ORDER BY sampled_at DESC LIMIT 1",
        )
        .bind(subject_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        row.map(UsageRow::into_usage).transpose()
    }
}

fn subject_kind_str(kind: QuotaSubject) -> &'static str {
    match kind {
        QuotaSubject::User => "user",
        QuotaSubject::Site => "site",
    }
}

fn parse_subject_kind(s: &str) -> Result<QuotaSubject, QuotaError> {
    match s {
        "user" => Ok(QuotaSubject::User),
        "site" => Ok(QuotaSubject::Site),
        other => Err(QuotaError::Persistence(format!(
            "unknown subject kind `{other}`"
        ))),
    }
}

fn dimension_str(d: QuotaDimension) -> String {
    match d {
        QuotaDimension::Disk => "disk".to_string(),
        QuotaDimension::Bandwidth => "bandwidth".to_string(),
        QuotaDimension::Inodes => "inodes".to_string(),
    }
}

fn parse_dimension(s: &str) -> Result<QuotaDimension, QuotaError> {
    match s {
        "disk" => Ok(QuotaDimension::Disk),
        "bandwidth" => Ok(QuotaDimension::Bandwidth),
        "inodes" => Ok(QuotaDimension::Inodes),
        other => Err(QuotaError::Persistence(format!(
            "unknown dimension `{other}`"
        ))),
    }
}

#[derive(sqlx::FromRow)]
struct PolicyRow {
    id: String,
    subject_kind: String,
    subject_id: String,
    disk_soft_bytes: i64,
    disk_hard_bytes: i64,
    disk_grace_days: i64,
    bandwidth_soft_bytes: i64,
    bandwidth_hard_bytes: i64,
    bandwidth_grace_days: i64,
    inodes_soft: i64,
    inodes_hard: i64,
    inodes_grace_days: i64,
    max_file_size_bytes: Option<i64>,
    cpu_shares: Option<i64>,
    created_at: String,
    updated_at: String,
}

impl PolicyRow {
    fn into_policy(self) -> Result<QuotaPolicy, QuotaError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| QuotaError::Persistence(format!("bad id: {e}")))?;
        let subject_kind = parse_subject_kind(&self.subject_kind)?;
        let subject_id = Uuid::parse_str(&self.subject_id)
            .map_err(|e| QuotaError::Persistence(format!("bad subject_id: {e}")))?;
        let disk = QuotaLimit {
            soft_bytes: self.disk_soft_bytes as u64,
            hard_bytes: self.disk_hard_bytes as u64,
            grace_days: self.disk_grace_days as u32,
        };
        let bandwidth = QuotaLimit {
            soft_bytes: self.bandwidth_soft_bytes as u64,
            hard_bytes: self.bandwidth_hard_bytes as u64,
            grace_days: self.bandwidth_grace_days as u32,
        };
        let inodes = QuotaLimit {
            soft_bytes: self.inodes_soft as u64,
            hard_bytes: self.inodes_hard as u64,
            grace_days: self.inodes_grace_days as u32,
        };
        let created_at = parse_dt(&self.created_at)?;
        let updated_at = parse_dt(&self.updated_at)?;
        QuotaPolicy::restore(
            id,
            subject_kind,
            subject_id,
            disk,
            bandwidth,
            inodes,
            self.max_file_size_bytes.map(|v| v as u64),
            self.cpu_shares.map(|v| v as u32),
            created_at,
            updated_at,
        )
    }
}

#[derive(sqlx::FromRow)]
struct UsageRow {
    subject_id: String,
    sampled_at: String,
    disk_used_bytes: i64,
    disk_inodes_used: i64,
    bandwidth_used_bytes: i64,
    over_soft_json: String,
    over_hard_json: String,
}

impl UsageRow {
    fn into_usage(self) -> Result<QuotaUsage, QuotaError> {
        let subject_id = Uuid::parse_str(&self.subject_id)
            .map_err(|e| QuotaError::Persistence(format!("bad subject_id: {e}")))?;
        let sampled_at = parse_dt(&self.sampled_at)?;
        let over_soft: Vec<String> = serde_json::from_str(&self.over_soft_json)
            .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        let over_hard: Vec<String> = serde_json::from_str(&self.over_hard_json)
            .map_err(|e| QuotaError::Persistence(e.to_string()))?;
        Ok(QuotaUsage {
            subject_id,
            sampled_at,
            disk_used_bytes: self.disk_used_bytes as u64,
            disk_inodes_used: self.disk_inodes_used as u64,
            bandwidth_used_bytes_this_month: self.bandwidth_used_bytes as u64,
            over_soft: over_soft
                .into_iter()
                .map(|s| parse_dimension(&s))
                .collect::<Result<_, _>>()?,
            over_hard: over_hard
                .into_iter()
                .map(|s| parse_dimension(&s))
                .collect::<Result<_, _>>()?,
        })
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, QuotaError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| QuotaError::Persistence(format!("bad timestamp `{s}`: {e}")))
}

// Suppress unused warnings for the unused context in `exists`.
#[allow(dead_code)]
fn _id_unused(_id: Uuid) {}

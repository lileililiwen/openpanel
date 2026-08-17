//! SQLite-backed repository for the container runtime bounded
//! context. All Uuids are stored as TEXT (RFC-4122 form);
//! timestamps are stored as RFC-3339 strings.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    ContainerMetrics, ContainerQuota, ContainerRuntimeRepository, NetworkEgressAccount,
    RegistryCredential, RegistryCredentialId, common::error::RepoError,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid datetime `{s}`: {e}")))
}

fn parse_uuid(s: &str, label: &str) -> Result<Uuid, RepoError> {
    Uuid::parse_str(s).map_err(|e| RepoError::new(format!("invalid {label} `{s}`: {e}")))
}

/// SQLite-backed repository implementation.
#[derive(Clone)]
pub struct SqliteContainerRuntimeRepository {
    pool: Pool<Sqlite>,
}

impl SqliteContainerRuntimeRepository {
    /// Construct a new repository bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Borrow the underlying pool. Exposed for the bounded
    /// context's own background task (metrics retention); Not
    /// part of the domain trait surface.
    pub fn pool(&self) -> Pool<Sqlite> {
        self.pool.clone()
    }
}

#[async_trait]
impl ContainerRuntimeRepository for SqliteContainerRuntimeRepository {
    async fn save_quota(&self, quota: &ContainerQuota) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO container_quotas
                (user_id, max_concurrent, max_total, cpu_pct_max,
                 memory_bytes_max, egress_bytes_per_month, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(quota.user_id.to_string())
        .bind(quota.max_concurrent as i64)
        .bind(quota.max_total as i64)
        .bind(quota.cpu_pct_max as i64)
        .bind(quota.memory_bytes_max as i64)
        .bind(quota.egress_bytes_per_month as i64)
        .bind(quota.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_quota(&self, user_id: Uuid) -> Result<Option<ContainerQuota>, RepoError> {
        let row: Option<QuotaRow> = sqlx::query_as(
            "SELECT user_id, max_concurrent, max_total, cpu_pct_max,
                    memory_bytes_max, egress_bytes_per_month, updated_at
             FROM container_quotas WHERE user_id = ?",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_quota).transpose()
    }

    async fn save_metrics(&self, metrics: &ContainerMetrics) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO container_metrics_samples
                (container_id, user_id, cpu_pct, memory_bytes,
                 net_rx, net_tx, exits, sampled_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(metrics.container_id.to_string())
        .bind(metrics.user_id.to_string())
        .bind(metrics.cpu_pct as i64)
        .bind(metrics.memory_bytes as i64)
        .bind(metrics.net_rx as i64)
        .bind(metrics.net_tx as i64)
        .bind(metrics.exits as i64)
        .bind(metrics.sampled_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_metrics(
        &self,
        container_id: Uuid,
        limit: u32,
    ) -> Result<Vec<ContainerMetrics>, RepoError> {
        let limit_i = limit.min(10_000) as i32;
        let rows: Vec<MetricsRow> = sqlx::query_as(
            "SELECT container_id, user_id, cpu_pct, memory_bytes,
                    net_rx, net_tx, exits, sampled_at
             FROM container_metrics_samples
             WHERE container_id = ?
             ORDER BY sampled_at ASC
             LIMIT ?",
        )
        .bind(container_id.to_string())
        .bind(limit_i)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_metrics).collect()
    }

    async fn trim_metrics(
        &self,
        container_id: Uuid,
        before: DateTime<Utc>,
    ) -> Result<u64, RepoError> {
        let res = sqlx::query(
            "DELETE FROM container_metrics_samples
             WHERE container_id = ? AND sampled_at < ?",
        )
        .bind(container_id.to_string())
        .bind(before.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn save_egress(&self, account: &NetworkEgressAccount) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO network_egress_accounts
                (user_id, container_id, month, bytes)
             VALUES (?, ?, ?, ?)",
        )
        .bind(account.user_id.to_string())
        .bind(match account.container_id {
            Some(u) => u.to_string(),
            None => String::new(),
        })
        .bind(&account.month)
        .bind(account.bytes as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_egress(
        &self,
        user_id: Uuid,
        container_id: Option<Uuid>,
        month: &str,
    ) -> Result<Option<NetworkEgressAccount>, RepoError> {
        match container_id {
            Some(id) => {
                let row: Option<EgressRow> = sqlx::query_as(
                    "SELECT user_id, container_id, month, bytes
                     FROM network_egress_accounts
                     WHERE user_id = ? AND container_id = ? AND month = ?",
                )
                .bind(user_id.to_string())
                .bind(id.to_string())
                .bind(month)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| RepoError::new(e.to_string()))?;
                row.map(row_to_egress).transpose()
            }
            None => {
                let row: Option<EgressRow> = sqlx::query_as(
                    "SELECT user_id, container_id, month, bytes
                     FROM network_egress_accounts
                     WHERE user_id = ? AND container_id = '' AND month = ?",
                )
                .bind(user_id.to_string())
                .bind(month)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| RepoError::new(e.to_string()))?;
                row.map(row_to_egress).transpose()
            }
        }
    }

    async fn add_egress(
        &self,
        user_id: Uuid,
        container_id: Option<Uuid>,
        month: &str,
        delta: u64,
    ) -> Result<u64, RepoError> {
        let capped = delta.min(i64::MAX as u64) as i64;
        let container_id_str = match container_id {
            Some(u) => u.to_string(),
            None => String::new(),
        };
        let res = sqlx::query(
            "INSERT INTO network_egress_accounts
                (user_id, container_id, month, bytes)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(user_id, container_id, month)
             DO UPDATE SET bytes = MIN(bytes + excluded.bytes, 9223372036854775807)",
        )
        .bind(user_id.to_string())
        .bind(&container_id_str)
        .bind(month)
        .bind(capped)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new("add_egress: no rows affected"));
        }
        self.get_egress(user_id, container_id, month)
            .await?
            .map(|a| a.bytes)
            .ok_or_else(|| RepoError::new("add_egress: missing row after upsert"))
    }

    async fn save_credential(&self, credential: &RegistryCredential) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO registry_credentials
                (id, user_id, registry, username,
                 encrypted_secret, created_at, last_used_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(credential.id.to_string())
        .bind(credential.user_id.to_string())
        .bind(&credential.registry)
        .bind(&credential.username)
        .bind(&credential.encrypted_secret)
        .bind(credential.created_at.to_rfc3339())
        .bind(credential.last_used_at.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_credential_by_id(
        &self,
        id: RegistryCredentialId,
    ) -> Result<Option<RegistryCredential>, RepoError> {
        let row: Option<CredentialRow> = sqlx::query_as(
            "SELECT id, user_id, registry, username,
                    encrypted_secret, created_at, last_used_at
             FROM registry_credentials WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_credential).transpose()
    }

    async fn get_credential(
        &self,
        user_id: Uuid,
        registry: &str,
    ) -> Result<Option<RegistryCredential>, RepoError> {
        let row: Option<CredentialRow> = sqlx::query_as(
            "SELECT id, user_id, registry, username,
                    encrypted_secret, created_at, last_used_at
             FROM registry_credentials
             WHERE user_id = ? AND registry = ?",
        )
        .bind(user_id.to_string())
        .bind(registry)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_credential).transpose()
    }

    async fn list_credentials(&self, user_id: Uuid) -> Result<Vec<RegistryCredential>, RepoError> {
        let rows: Vec<CredentialRow> = sqlx::query_as(
            "SELECT id, user_id, registry, username,
                    encrypted_secret, created_at, last_used_at
             FROM registry_credentials
             WHERE user_id = ?
             ORDER BY created_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_credential).collect()
    }

    async fn delete_credential(&self, id: RegistryCredentialId) -> Result<bool, RepoError> {
        let res = sqlx::query("DELETE FROM registry_credentials WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(res.rows_affected() > 0)
    }

    async fn touch_credential(
        &self,
        id: RegistryCredentialId,
        at: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        let res = sqlx::query("UPDATE registry_credentials SET last_used_at = ? WHERE id = ?")
            .bind(at.to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new(format!("credential {id} not found")));
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct QuotaRow {
    user_id: String,
    max_concurrent: i64,
    max_total: i64,
    cpu_pct_max: i64,
    memory_bytes_max: i64,
    egress_bytes_per_month: i64,
    updated_at: String,
}

fn row_to_quota(row: QuotaRow) -> Result<ContainerQuota, RepoError> {
    let user_id = parse_uuid(&row.user_id, "user_id")?;
    Ok(ContainerQuota {
        user_id,
        max_concurrent: row.max_concurrent.max(0) as u32,
        max_total: row.max_total.max(0) as u32,
        cpu_pct_max: row.cpu_pct_max.clamp(0, 255) as u8,
        memory_bytes_max: row.memory_bytes_max.max(0) as u64,
        egress_bytes_per_month: row.egress_bytes_per_month.max(0) as u64,
        updated_at: parse_dt(&row.updated_at)?,
    })
}

#[derive(sqlx::FromRow)]
struct MetricsRow {
    container_id: String,
    user_id: String,
    cpu_pct: i64,
    memory_bytes: i64,
    net_rx: i64,
    net_tx: i64,
    exits: i64,
    sampled_at: String,
}

fn row_to_metrics(row: MetricsRow) -> Result<ContainerMetrics, RepoError> {
    Ok(ContainerMetrics {
        user_id: parse_uuid(&row.user_id, "user_id")?,
        container_id: parse_uuid(&row.container_id, "container_id")?,
        cpu_pct: row.cpu_pct.clamp(0, 65535) as u16,
        memory_bytes: row.memory_bytes.max(0) as u64,
        net_rx: row.net_rx.max(0) as u64,
        net_tx: row.net_tx.max(0) as u64,
        exits: row.exits.max(0) as u32,
        sampled_at: parse_dt(&row.sampled_at)?,
    })
}

#[derive(sqlx::FromRow)]
struct EgressRow {
    user_id: String,
    container_id: String,
    month: String,
    bytes: i64,
}

fn row_to_egress(row: EgressRow) -> Result<NetworkEgressAccount, RepoError> {
    let container_id = match row.container_id.as_str() {
        "" => None,
        s => Some(parse_uuid(s, "container_id")?),
    };
    Ok(NetworkEgressAccount {
        user_id: parse_uuid(&row.user_id, "user_id")?,
        container_id,
        month: row.month,
        bytes: row.bytes.max(0) as u64,
    })
}

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: String,
    user_id: String,
    registry: String,
    username: String,
    encrypted_secret: String,
    created_at: String,
    last_used_at: Option<String>,
}

fn row_to_credential(row: CredentialRow) -> Result<RegistryCredential, RepoError> {
    Ok(RegistryCredential {
        id: parse_uuid(&row.id, "credential id")?,
        user_id: parse_uuid(&row.user_id, "user_id")?,
        registry: row.registry,
        username: row.username,
        encrypted_secret: row.encrypted_secret,
        created_at: parse_dt(&row.created_at)?,
        last_used_at: row.last_used_at.as_deref().map(parse_dt).transpose()?,
    })
}

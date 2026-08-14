//! SQLite-backed `PluginRegistry` adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::common::error::RepoError;
use openpanel_domain::{
    PluginError, PluginId, PluginRecord, PluginRegistry, PluginStatus, PluginVersion, PublisherKey,
};
use sqlx::{Pool, Sqlite};

/// SQLite-backed `PluginRegistry`.
#[derive(Clone)]
pub struct SqlitePluginRegistry {
    pool: Pool<Sqlite>,
}

impl SqlitePluginRegistry {
    /// Construct a new registry bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PluginRegistry for SqlitePluginRegistry {
    async fn insert(&self, record: &PluginRecord) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO installed_plugins
                (id, version, publisher, status, installed_at,
                 enabled_at, last_error)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(record.id.as_str())
        .bind(record.version.as_str())
        .bind(record.publisher.as_str())
        .bind(plugin_status_str(record.status))
        .bind(record.installed_at.to_rfc3339())
        .bind(record.enabled_at.map(|t| t.to_rfc3339()))
        .bind(record.last_error.clone())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find(&self, id: &PluginId) -> Result<Option<PluginRecord>, RepoError> {
        let row: Option<PluginRow> = sqlx::query_as(
            "SELECT id, version, publisher, status, installed_at,
                    enabled_at, last_error
             FROM installed_plugins WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_record).transpose()
    }

    async fn update_status(
        &self,
        id: &PluginId,
        status: PluginStatus,
        error: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), RepoError> {
        let result = sqlx::query(
            "UPDATE installed_plugins
             SET status = ?, last_error = ?,
                 enabled_at = CASE
                     WHEN ? = 'enabled' THEN ?
                     ELSE enabled_at
                 END
             WHERE id = ?",
        )
        .bind(plugin_status_str(status))
        .bind(error)
        .bind(plugin_status_str(status))
        .bind(at.to_rfc3339())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(RepoError::new(format!(
                "plugin {} not installed",
                id.as_str()
            )));
        }
        Ok(())
    }

    async fn delete(&self, id: &PluginId) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM installed_plugins WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<PluginRecord>, RepoError> {
        let rows: Vec<PluginRow> = sqlx::query_as(
            "SELECT id, version, publisher, status, installed_at,
                    enabled_at, last_error
             FROM installed_plugins ORDER BY installed_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_record).collect()
    }
}

#[derive(sqlx::FromRow)]
struct PluginRow {
    id: String,
    version: String,
    publisher: String,
    status: String,
    installed_at: String,
    enabled_at: Option<String>,
    last_error: Option<String>,
}

fn row_to_record(row: PluginRow) -> Result<PluginRecord, RepoError> {
    let id = PluginId::new(row.id).map_err(|e: PluginError| RepoError::new(e.to_string()))?;
    let version = PluginVersion::new(row.version)
        .map_err(|e: PluginError| RepoError::new(e.to_string()))?;
    let publisher = PublisherKey::new(row.publisher)
        .map_err(|e: PluginError| RepoError::new(e.to_string()))?;
    let status = parse_status(&row.status)
        .map_err(|e: PluginError| RepoError::new(e.to_string()))?;
    let installed_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&row.installed_at)
        .map_err(|e| RepoError::new(format!("installed_at: {e}")))?
        .with_timezone(&Utc);
    let enabled_at = match row.enabled_at {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| RepoError::new(format!("enabled_at: {e}")))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    Ok(PluginRecord {
        id,
        version,
        publisher,
        status,
        installed_at,
        enabled_at,
        last_error: row.last_error,
    })
}

fn plugin_status_str(status: PluginStatus) -> &'static str {
    match status {
        PluginStatus::Installed => "installed",
        PluginStatus::Enabled => "enabled",
        PluginStatus::Disabled => "disabled",
        PluginStatus::Failed => "failed",
    }
}

fn parse_status(s: &str) -> Result<PluginStatus, PluginError> {
    match s {
        "installed" => Ok(PluginStatus::Installed),
        "enabled" => Ok(PluginStatus::Enabled),
        "disabled" => Ok(PluginStatus::Disabled),
        "failed" => Ok(PluginStatus::Failed),
        other => Err(PluginError::InvalidManifest(format!(
            "unknown plugin status `{other}`"
        ))),
    }
}
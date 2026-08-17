//! SQLite adapter for the WordPress toolkit bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{RepoError, WpCacheMode, WpRepository, WpSite, WpUpdateResult, WpUpdateSet};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `WpRepository`.
#[derive(Clone)]
pub struct SqliteWpRepository {
    pool: SqlitePool,
}

impl SqliteWpRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl WpRepository for SqliteWpRepository {
    async fn save_site(&self, site: &WpSite) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO wp_sites \
             (id, site_id, wp_root, core_version, last_snapshot_db, last_snapshot_files, cache_mode, registered_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(site.id.to_string())
        .bind(site.site_id.to_string())
        .bind(&site.wp_root)
        .bind(&site.core_version)
        .bind(&site.last_snapshot_db)
        .bind(&site.last_snapshot_files)
        .bind(site.cache_mode.as_str())
        .bind(site.registered_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_site(&self, site_id: Uuid) -> Result<Option<WpSite>, RepoError> {
        let row = sqlx::query(
            "SELECT id, site_id, wp_root, core_version, last_snapshot_db, last_snapshot_files, cache_mode, registered_at \
             FROM wp_sites WHERE site_id = ?",
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_site).transpose()
    }

    async fn save_update_run(&self, result: &WpUpdateResult) -> Result<(), RepoError> {
        let updates_json =
            serde_json::to_string(&result.updates).map_err(|e| RepoError::new(e.to_string()))?;
        let completed_at = result.completed_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO wp_update_runs \
             (id, site_id, started_at, completed_at, updates_json, success, rolled_back, message) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(result.id.to_string())
        .bind(result.site_id.to_string())
        .bind(result.started_at.to_rfc3339())
        .bind(completed_at)
        .bind(updates_json)
        .bind(if result.success { 1 } else { 0 })
        .bind(if result.rolled_back { 1 } else { 0 })
        .bind(&result.message)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_update_runs(
        &self,
        site_id: Uuid,
        limit: u32,
    ) -> Result<Vec<WpUpdateResult>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, site_id, started_at, completed_at, updates_json, success, rolled_back, message \
             FROM wp_update_runs WHERE site_id = ? ORDER BY started_at DESC LIMIT ?",
        )
        .bind(site_id.to_string())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_run).collect()
    }
}

fn decode_site(row: sqlx::sqlite::SqliteRow) -> Result<WpSite, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let wp_root: String = row.try_get("wp_root").map_err(map_sqlx)?;
    let core_version: String = row.try_get("core_version").map_err(map_sqlx)?;
    let last_snapshot_db: Option<String> = row.try_get("last_snapshot_db").map_err(map_sqlx)?;
    let last_snapshot_files: Option<String> =
        row.try_get("last_snapshot_files").map_err(map_sqlx)?;
    let cache_mode: String = row.try_get("cache_mode").map_err(map_sqlx)?;
    let registered_at: String = row.try_get("registered_at").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let cache_mode = match cache_mode.as_str() {
        "off" => WpCacheMode::Off,
        "standard" => WpCacheMode::Standard,
        "aggressive" => WpCacheMode::Aggressive,
        other => return Err(RepoError::new(format!("unknown cache mode: {other}"))),
    };
    let registered_at = parse_ts(&registered_at)?;
    Ok(WpSite {
        id,
        site_id,
        wp_root,
        core_version,
        last_snapshot_db,
        last_snapshot_files,
        cache_mode,
        registered_at,
    })
}

fn decode_run(row: sqlx::sqlite::SqliteRow) -> Result<WpUpdateResult, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let started_at: String = row.try_get("started_at").map_err(map_sqlx)?;
    let completed_at: Option<String> = row.try_get("completed_at").map_err(map_sqlx)?;
    let updates_json: String = row.try_get("updates_json").map_err(map_sqlx)?;
    let success: i64 = row.try_get("success").map_err(map_sqlx)?;
    let rolled_back: i64 = row.try_get("rolled_back").map_err(map_sqlx)?;
    let message: String = row.try_get("message").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let site_id = Uuid::parse_str(&site_id).map_err(|e| RepoError::new(e.to_string()))?;
    let started_at = parse_ts(&started_at)?;
    let completed_at = completed_at.as_deref().map(parse_ts).transpose()?;
    let updates: Vec<WpUpdateSet> = serde_json::from_str(&updates_json)
        .map_err(|e| RepoError::new(format!("invalid updates_json: {e}")))?;
    Ok(WpUpdateResult {
        id,
        site_id,
        started_at,
        completed_at,
        updates,
        success: success != 0,
        rolled_back: rolled_back != 0,
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

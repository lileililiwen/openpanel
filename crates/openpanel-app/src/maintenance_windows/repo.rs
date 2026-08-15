//! SQLite adapter for the maintenance windows bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    DestructiveActionClass, MaintenanceError, MaintenanceOverride, MaintenanceRepository,
    MaintenanceWindow, RepoError,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `MaintenanceRepository`.
#[derive(Clone)]
pub struct SqliteMaintenanceRepository {
    pool: SqlitePool,
}

impl SqliteMaintenanceRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MaintenanceRepository for SqliteMaintenanceRepository {
    async fn save_window(&self, window: &MaintenanceWindow) -> Result<(), RepoError> {
        let blocked = serde_json::to_string(&window.blocked_classes)
            .map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO maintenance_windows \
             (id, label, starts_at, ends_at, blocked_classes, created_at, created_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(window.id.to_string())
        .bind(&window.label)
        .bind(window.starts_at.to_rfc3339())
        .bind(window.ends_at.to_rfc3339())
        .bind(blocked)
        .bind(window.created_at.to_rfc3339())
        .bind(window.created_by.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_windows(&self) -> Result<Vec<MaintenanceWindow>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, label, starts_at, ends_at, blocked_classes, created_at, created_by \
             FROM maintenance_windows ORDER BY starts_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_window).collect()
    }

    async fn get_window(&self, id: Uuid) -> Result<Option<MaintenanceWindow>, RepoError> {
        let row = sqlx::query(
            "SELECT id, label, starts_at, ends_at, blocked_classes, created_at, created_by \
             FROM maintenance_windows WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_window).transpose()
    }

    async fn delete_window(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM maintenance_windows WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_override(&self, override_: &MaintenanceOverride) -> Result<(), RepoError> {
        let consumed_at = override_.consumed_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO maintenance_overrides \
             (id, target_class, reason, ttl_secs, created_at, expires_at, consumed_at, issued_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(override_.id.to_string())
        .bind(override_.target_class.as_str())
        .bind(&override_.reason)
        .bind(override_.ttl_secs as i64)
        .bind(override_.created_at.to_rfc3339())
        .bind(override_.expires_at.to_rfc3339())
        .bind(consumed_at)
        .bind(override_.issued_by.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_override(
        &self,
        id: Uuid,
    ) -> Result<Option<MaintenanceOverride>, RepoError> {
        let row = sqlx::query(
            "SELECT id, target_class, reason, ttl_secs, created_at, expires_at, consumed_at, issued_by \
             FROM maintenance_overrides WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_override).transpose()
    }
}

fn decode_window(row: sqlx::sqlite::SqliteRow) -> Result<MaintenanceWindow, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let label: String = row.try_get("label").map_err(map_sqlx)?;
    let starts_at: String = row.try_get("starts_at").map_err(map_sqlx)?;
    let ends_at: String = row.try_get("ends_at").map_err(map_sqlx)?;
    let blocked_classes: String = row.try_get("blocked_classes").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let created_by: String = row.try_get("created_by").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let created_by = Uuid::parse_str(&created_by).map_err(|e| RepoError::new(e.to_string()))?;
    let blocked_classes: Vec<DestructiveActionClass> = serde_json::from_str(&blocked_classes)
        .map_err(|e| RepoError::new(format!("invalid blocked_classes: {e}")))?;
    Ok(MaintenanceWindow {
        id,
        label,
        starts_at: parse_ts(&starts_at)?,
        ends_at: parse_ts(&ends_at)?,
        blocked_classes,
        created_at: parse_ts(&created_at)?,
        created_by,
    })
}

fn decode_override(row: sqlx::sqlite::SqliteRow) -> Result<MaintenanceOverride, RepoError> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let target_class: String = row.try_get("target_class").map_err(map_sqlx)?;
    let reason: String = row.try_get("reason").map_err(map_sqlx)?;
    let ttl_secs: i64 = row.try_get("ttl_secs").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let expires_at: String = row.try_get("expires_at").map_err(map_sqlx)?;
    let consumed_at: Option<String> = row.try_get("consumed_at").map_err(map_sqlx)?;
    let issued_by: String = row.try_get("issued_by").map_err(map_sqlx)?;
    let id = Uuid::parse_str(&id).map_err(|e| RepoError::new(e.to_string()))?;
    let issued_by = Uuid::parse_str(&issued_by).map_err(|e| RepoError::new(e.to_string()))?;
    let target_class = match target_class.as_str() {
        "package_install" => DestructiveActionClass::PackageInstall,
        "schema_migration" => DestructiveActionClass::SchemaMigration,
        "filesystem_rewrite" => DestructiveActionClass::FilesystemRewrite,
        other => return Err(RepoError::new(format!("unknown class: {other}"))),
    };
    Ok(MaintenanceOverride {
        id,
        target_class,
        reason,
        ttl_secs: ttl_secs as u32,
        created_at: parse_ts(&created_at)?,
        expires_at: parse_ts(&expires_at)?,
        consumed_at: consumed_at.as_deref().map(parse_ts).transpose()?,
        issued_by,
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
//! SQLite bounded health history.
use async_trait::async_trait;
use openpanel_domain::system_services::Health;
use sqlx::{Pool, Row, Sqlite};

use super::{ServiceHealthRepository, ServiceManagerError};
/// SQLite health history reader.
pub struct SqliteServiceHealthRepository {
    pool: Pool<Sqlite>,
}
impl SqliteServiceHealthRepository {
    /// Construct over a shared SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl ServiceHealthRepository for SqliteServiceHealthRepository {
    async fn history(
        &self,
        service_id: &str,
        limit: usize,
    ) -> Result<Vec<Health>, ServiceManagerError> {
        let rows=sqlx::query("SELECT health FROM service_health_history WHERE service_id=? ORDER BY observed_at DESC LIMIT ?").bind(service_id).bind(i64::try_from(limit).unwrap_or(500)).fetch_all(&self.pool).await.map_err(|_|ServiceManagerError::Persistence)?;
        Ok(rows
            .into_iter()
            .map(|row| match row.get::<String, _>(0).as_str() {
                "healthy" => Health::Healthy,
                "failed" => Health::Failed,
                _ => Health::Unknown,
            })
            .collect())
    }

    async fn record(&self, service_id: &str, health: Health) -> Result<(), ServiceManagerError> {
        let value = match health {
            Health::Healthy => "healthy",
            Health::Failed => "failed",
            Health::Unknown => "unknown",
        };
        sqlx::query(
            "INSERT INTO service_health_history(service_id,health,observed_at) VALUES(?,?,?)",
        )
        .bind(service_id)
        .bind(value)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|_| ServiceManagerError::Persistence)?;
        sqlx::query("DELETE FROM service_health_history WHERE id IN (SELECT id FROM service_health_history WHERE service_id=? ORDER BY observed_at DESC LIMIT -1 OFFSET 1000)").bind(service_id).execute(&self.pool).await.map_err(|_|ServiceManagerError::Persistence)?;
        Ok(())
    }
}

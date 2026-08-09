//! SQLite security state adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::security::{FirewallRule, LoginKey, NetworkCidr, TemporaryBlock};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use super::{SecurityRepository, SecurityServiceError};

/// SQLite-backed firewall drafts and login abuse state.
pub struct SqliteSecurityRepository {
    pool: Pool<Sqlite>,
}
impl SqliteSecurityRepository {
    /// Construct over a shared pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SecurityRepository for SqliteSecurityRepository {
    async fn list_rules(&self) -> Result<Vec<FirewallRule>, SecurityServiceError> {
        let rows = sqlx::query("SELECT payload FROM security_firewall_rules ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.into_iter()
            .map(|row| {
                serde_json::from_str(&row.get::<String, _>(0))
                    .map_err(|_| SecurityServiceError::Persistence)
            })
            .collect()
    }

    async fn get_rule(&self, id: Uuid) -> Result<Option<FirewallRule>, SecurityServiceError> {
        sqlx::query("SELECT payload FROM security_firewall_rules WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .map(|row| {
                serde_json::from_str(&row.get::<String, _>(0))
                    .map_err(|_| SecurityServiceError::Persistence)
            })
            .transpose()
    }

    async fn save_rule(&self, rule: &FirewallRule) -> Result<(), SecurityServiceError> {
        let payload = serde_json::to_string(rule).map_err(|_| SecurityServiceError::Persistence)?;
        sqlx::query("INSERT INTO security_firewall_rules (id,payload) VALUES (?,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload").bind(rule.id().to_string()).bind(payload).execute(&self.pool).await.map_err(db)?;
        Ok(())
    }

    async fn delete_rule(&self, id: Uuid) -> Result<(), SecurityServiceError> {
        sqlx::query("DELETE FROM security_firewall_rules WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn replace_rules(&self, rules: &[FirewallRule]) -> Result<(), SecurityServiceError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM security_firewall_rules")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for rule in rules {
            let payload =
                serde_json::to_string(rule).map_err(|_| SecurityServiceError::Persistence)?;
            sqlx::query("INSERT INTO security_firewall_rules(id,payload) VALUES(?,?)")
                .bind(rule.id().to_string())
                .bind(payload)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn save_failure(
        &self,
        key: &LoginKey,
        at: DateTime<Utc>,
    ) -> Result<(), SecurityServiceError> {
        sqlx::query("INSERT INTO security_login_failures(key,occurred_at) VALUES(?,?)")
            .bind(key.storage_key())
            .bind(at.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn failure_count(
        &self,
        key: &LoginKey,
        since: DateTime<Utc>,
    ) -> Result<u32, SecurityServiceError> {
        let value: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM security_login_failures WHERE key=? AND occurred_at>=?",
        )
        .bind(key.storage_key())
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(u32::try_from(value).unwrap_or(u32::MAX))
    }

    async fn save_block(&self, block: &TemporaryBlock) -> Result<(), SecurityServiceError> {
        let payload =
            serde_json::to_string(block).map_err(|_| SecurityServiceError::Persistence)?;
        sqlx::query("INSERT INTO security_blocks(key,payload) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET payload=excluded.payload").bind(block.key().storage_key()).bind(payload).execute(&self.pool).await.map_err(db)?;
        Ok(())
    }

    async fn clear_failures(&self, key: &LoginKey) -> Result<(), SecurityServiceError> {
        sqlx::query("DELETE FROM security_login_failures WHERE key=?")
            .bind(key.storage_key())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn list_blocks(&self) -> Result<Vec<TemporaryBlock>, SecurityServiceError> {
        let rows = sqlx::query("SELECT payload FROM security_blocks ORDER BY key")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.into_iter()
            .map(|row| {
                serde_json::from_str(&row.get::<String, _>(0))
                    .map_err(|_| SecurityServiceError::Persistence)
            })
            .collect()
    }

    async fn unblock(&self, key: &LoginKey, at: DateTime<Utc>) -> Result<(), SecurityServiceError> {
        let row = sqlx::query("SELECT payload FROM security_blocks WHERE key=?")
            .bind(key.storage_key())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(SecurityServiceError::NotFound)?;
        let mut block: TemporaryBlock = serde_json::from_str(&row.get::<String, _>(0))
            .map_err(|_| SecurityServiceError::Persistence)?;
        block.unblock(at);
        self.save_block(&block).await
    }

    async fn active_block(
        &self,
        key: &LoginKey,
        at: DateTime<Utc>,
    ) -> Result<Option<TemporaryBlock>, SecurityServiceError> {
        let row = sqlx::query("SELECT payload FROM security_blocks WHERE key=?")
            .bind(key.storage_key())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let block = row
            .map(|row| {
                serde_json::from_str::<TemporaryBlock>(&row.get::<String, _>(0))
                    .map_err(|_| SecurityServiceError::Persistence)
            })
            .transpose()?;
        Ok(block.filter(|block| block.is_active(at)))
    }

    async fn list_allowlists(&self) -> Result<Vec<NetworkCidr>, SecurityServiceError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT network FROM security_allowlists ORDER BY network",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter()
            .map(|network| NetworkCidr::parse(&network).map_err(Into::into))
            .collect()
    }

    async fn add_allowlist(&self, network: NetworkCidr) -> Result<(), SecurityServiceError> {
        sqlx::query("INSERT OR IGNORE INTO security_allowlists(network) VALUES(?)")
            .bind(network.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn remove_allowlist(&self, network: NetworkCidr) -> Result<(), SecurityServiceError> {
        sqlx::query("DELETE FROM security_allowlists WHERE network=?")
            .bind(network.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}
fn db(_error: sqlx::Error) -> SecurityServiceError {
    SecurityServiceError::Persistence
}

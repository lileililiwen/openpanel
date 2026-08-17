//! SQLite-backed adapter for the webmail bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{WebmailError, WebmailRepository, WebmailSessionToken};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed repository.
#[derive(Clone)]
pub struct SqliteWebmailRepository {
    pool: Pool<Sqlite>,
}

impl SqliteWebmailRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[async_trait]
impl WebmailRepository for SqliteWebmailRepository {
    async fn insert_session(&self, session: &WebmailSessionToken) -> Result<(), WebmailError> {
        sqlx::query(
            "INSERT OR REPLACE INTO webmail_session_tokens
             (id, mailbox, token, password_cipher, rotated_original_cipher, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(session.id().to_string())
        .bind(session.mailbox())
        .bind(session.token())
        .bind(session.password_cipher())
        .bind(session.rotated_original_cipher())
        .bind(session.created_at().to_rfc3339())
        .bind(session.expires_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| WebmailError::Persistence(format!("insert session: {e}")))?;
        Ok(())
    }

    async fn find_session(&self, token: &str) -> Result<Option<WebmailSessionToken>, WebmailError> {
        let row = sqlx::query(
            "SELECT id, mailbox, token, password_cipher, rotated_original_cipher,
                    created_at, expires_at
             FROM webmail_session_tokens WHERE token = ?1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| WebmailError::Persistence(format!("find session: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let id: String = row.get("id");
        let mailbox: String = row.get("mailbox");
        let token: String = row.get("token");
        let password_cipher: String = row.get("password_cipher");
        let rotated_original_cipher: String = row.get("rotated_original_cipher");
        let created_at: String = row.get("created_at");
        let id = Uuid::parse_str(&id).map_err(|e| WebmailError::Persistence(format!("id: {e}")))?;
        Ok(Some(WebmailSessionToken::new(
            id,
            mailbox,
            token,
            password_cipher,
            rotated_original_cipher,
            parse_ts(&created_at),
        )?))
    }

    async fn revoke_session(&self, id: Uuid) -> Result<(), WebmailError> {
        sqlx::query("DELETE FROM webmail_session_tokens WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| WebmailError::Persistence(format!("revoke session: {e}")))?;
        Ok(())
    }

    async fn revoke_all_for_mailbox(&self, mailbox: &str) -> Result<(), WebmailError> {
        sqlx::query("DELETE FROM webmail_session_tokens WHERE mailbox = ?1")
            .bind(mailbox)
            .execute(&self.pool)
            .await
            .map_err(|e| WebmailError::Persistence(format!("revoke all: {e}")))?;
        Ok(())
    }
}

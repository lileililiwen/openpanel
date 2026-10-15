//! SQLite persistence for the staging capture inbox.
//!
//! Rows store the body already encrypted (`nonce:ciphertext` hex,
//! AES-256-GCM under the panel master key); this layer never sees or
//! returns plaintext bodies.

use sqlx::{Pool, Sqlite, sqlite::SqlitePool};
use uuid::Uuid;

use openpanel_domain::{CapturedMessage, CapturedMessageSummary, MailCaptureError};

/// One persisted row; `encrypted_body` is opaque secret material.
#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    site_id: String,
    slot_id: String,
    mailbox: String,
    rcpt_to: String,
    subject: String,
    encrypted_body: String,
    captured_at: String,
}

/// Storage adapter for captured messages.
pub struct SqliteMailCaptureRepository {
    pool: Pool<Sqlite>,
}

impl SqliteMailCaptureRepository {
    /// Build a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert one message (body already encrypted) and then enforce
    /// the per-slot FIFO retention cap.
    pub async fn insert(
        &self,
        message: &CapturedMessage,
        encrypted_body: &str,
        cap: i64,
    ) -> Result<(), MailCaptureError> {
        let slot_id = message.slot_id().to_string();
        sqlx::query(
            r#"INSERT INTO mail_capture_messages
                (id, site_id, slot_id, mailbox, rcpt_to, subject,
                 encrypted_body, captured_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(message.id().to_string())
        .bind(message.site_id().to_string())
        .bind(&slot_id)
        .bind(message.mailbox())
        .bind(message.recipient())
        .bind(message.subject())
        .bind(encrypted_body)
        .bind(message.captured_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        sqlx::query(
            r#"DELETE FROM mail_capture_messages
                WHERE slot_id = ?
                  AND id NOT IN (
                      SELECT id FROM mail_capture_messages
                       WHERE slot_id = ?
                       ORDER BY captured_at DESC, id DESC
                       LIMIT ?
                  )"#,
        )
        .bind(&slot_id)
        .bind(&slot_id)
        .bind(cap)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    /// Body-free rows for a site, newest first.
    pub async fn list_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<CapturedMessageSummary>, MailCaptureError> {
        let rows: Vec<MessageRow> = sqlx::query_as::<_, MessageRow>(
            "SELECT id, site_id, slot_id, mailbox, rcpt_to, subject, encrypted_body, captured_at
               FROM mail_capture_messages WHERE site_id = ?
              ORDER BY captured_at DESC",
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        rows.into_iter().map(row_summary).collect()
    }

    /// One full row (encrypted body included) within a site.
    pub async fn find(
        &self,
        site_id: Uuid,
        id: Uuid,
    ) -> Result<Option<EncodedMessage>, MailCaptureError> {
        let row = sqlx::query_as::<_, MessageRow>(
            "SELECT id, site_id, slot_id, mailbox, rcpt_to, subject, encrypted_body, captured_at
               FROM mail_capture_messages WHERE site_id = ? AND id = ?",
        )
        .bind(site_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        match row {
            Some(row) => Ok(Some(EncodedMessage::from_row(row)?)),
            None => Ok(None),
        }
    }

    /// Delete every row for a site; returns the removed count.
    pub async fn purge_site(&self, site_id: Uuid) -> Result<u32, MailCaptureError> {
        let result = sqlx::query("DELETE FROM mail_capture_messages WHERE site_id = ?")
            .bind(site_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(result.rows_affected() as u32)
    }

    /// Delete every row for a slot.
    pub async fn purge_slot(&self, slot_id: Uuid) -> Result<(), MailCaptureError> {
        sqlx::query("DELETE FROM mail_capture_messages WHERE slot_id = ?")
            .bind(slot_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(())
    }

    /// The capturing slot id for a site, when its staging slot exists
    /// with mail capture enabled.
    pub async fn capturing_slot(&self, site_id: Uuid) -> Result<Option<Uuid>, MailCaptureError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT id FROM staging_slots WHERE site_id = ? AND mail_capture = 1")
                .bind(site_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(internal)?;
        row.map(|(id,)| {
            Uuid::parse_str(&id)
                .map(Some)
                .map_err(|e| MailCaptureError::Internal(e.to_string()))
        })
        .unwrap_or(Ok(None))
    }
}

/// A stored message with its body still encrypted.
pub struct EncodedMessage {
    /// Message id.
    pub id: Uuid,
    /// Owning site.
    pub site_id: Uuid,
    /// Owning slot.
    pub slot_id: Uuid,
    /// Sender mailbox.
    pub mailbox: String,
    /// Recipient envelope.
    pub recipient: String,
    /// Subject line.
    pub subject: String,
    /// AES-GCM `nonce:ciphertext` hex. Secret material.
    pub encrypted_body: String,
    /// Capture timestamp (RFC 3339).
    pub captured_at: String,
}

impl EncodedMessage {
    fn from_row(row: MessageRow) -> Result<Self, MailCaptureError> {
        Ok(Self {
            id: parse_uuid(&row.id, "id")?,
            site_id: parse_uuid(&row.site_id, "site_id")?,
            slot_id: parse_uuid(&row.slot_id, "slot_id")?,
            mailbox: row.mailbox,
            recipient: row.rcpt_to,
            subject: row.subject,
            encrypted_body: row.encrypted_body,
            captured_at: row.captured_at,
        })
    }
}

fn row_summary(row: MessageRow) -> Result<CapturedMessageSummary, MailCaptureError> {
    Ok(CapturedMessageSummary {
        id: parse_uuid(&row.id, "id")?,
        site_id: parse_uuid(&row.site_id, "site_id")?,
        slot_id: parse_uuid(&row.slot_id, "slot_id")?,
        mailbox: row.mailbox,
        recipient: row.rcpt_to,
        subject: row.subject,
        captured_at: chrono::DateTime::parse_from_rfc3339(&row.captured_at)
            .map(|at| at.with_timezone(&chrono::Utc))
            .map_err(|e| MailCaptureError::Internal(format!("bad captured_at: {e}")))?,
    })
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, MailCaptureError> {
    Uuid::parse_str(value).map_err(|e| MailCaptureError::Internal(format!("bad {field}: {e}")))
}

fn internal(error: impl std::fmt::Display) -> MailCaptureError {
    MailCaptureError::Internal(error.to_string())
}

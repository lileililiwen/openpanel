//! Staging mail-capture service: the capture port implementation
//! plus the authorized inbox views. Bodies are encrypted at rest with
//! the panel master key and never appear in audit payloads or logs.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    CapturedMessage, CapturedMessageSummary, MailCaptureError, MailCapturePort, Role, Site,
    SiteRepository, User,
};
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use super::repo::SqliteMailCaptureRepository;
use crate::databases::crypto::{KEY_LEN, decrypt_from_storage, encrypt_to_storage};

/// Per-slot FIFO retention cap (oldest messages are evicted first).
pub const RETENTION_CAP_PER_SLOT: i64 = 1000;

/// Body-free inbox row for list surfaces.
pub type MailCaptureView = CapturedMessageSummary;

/// Detail view carrying the decrypted body. Secret material: callers
/// MUST render it only to an authorized owner and MUST NOT log it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadCapturedMessageView {
    /// Message id.
    pub id: Uuid,
    /// Sender mailbox.
    pub mailbox: String,
    /// Recipient envelope.
    pub recipient: String,
    /// Subject line.
    pub subject: String,
    /// Decrypted body.
    pub body: String,
    /// Capture timestamp.
    pub captured_at: DateTime<Utc>,
}

/// Staging capture inbox use cases.
pub struct MailCaptureService {
    repo: Arc<SqliteMailCaptureRepository>,
    sites: Arc<dyn SiteRepository>,
    master_key: [u8; KEY_LEN],
    audit: Arc<dyn AuditService>,
}

impl MailCaptureService {
    /// Compose the service over the shared pool, site lookup, the
    /// panel master key for body encryption, and audit.
    pub fn new(
        pool: Pool<Sqlite>,
        sites: Arc<dyn SiteRepository>,
        master_key: [u8; KEY_LEN],
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo: Arc::new(SqliteMailCaptureRepository::new(pool)),
            sites,
            master_key,
            audit,
        }
    }

    async fn site(&self, caller: &User, site_id: Uuid) -> Result<Site, MailCaptureError> {
        let site = self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(internal)?
            .ok_or(MailCaptureError::SiteNotFound)?;
        if caller.role() != Role::Owner && site.owner_id() != caller.id() {
            return Err(MailCaptureError::Forbidden);
        }
        Ok(site)
    }

    /// List a caller-accessible site's inbox (body-free).
    pub async fn list(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<Vec<MailCaptureView>, MailCaptureError> {
        self.site(caller, site_id).await?;
        self.repo.list_for_site(site_id).await
    }

    /// Read one message with the body decrypted, scoped to the site.
    pub async fn read(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
    ) -> Result<ReadCapturedMessageView, MailCaptureError> {
        self.site(caller, site_id).await?;
        let row = self
            .repo
            .find(site_id, id)
            .await?
            .ok_or(MailCaptureError::NotFound)?;
        let body = decrypt_from_storage(&self.master_key, &row.encrypted_body)
            .map_err(|e| MailCaptureError::Internal(e.to_string()))?;
        Ok(ReadCapturedMessageView {
            id: row.id,
            mailbox: row.mailbox,
            recipient: row.recipient,
            subject: row.subject,
            body,
            captured_at: parse_ts(&row.captured_at)?,
        })
    }

    /// Purge every captured message for a site, auditing the count.
    pub async fn purge(&self, caller: &User, site_id: Uuid) -> Result<u32, MailCaptureError> {
        self.site(caller, site_id).await?;
        let removed = self.repo.purge_site(site_id).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::MailCapturePurged,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({"removed": removed})),
            )
            .await;
        Ok(removed)
    }

    async fn store_for_slot(
        &self,
        slot_id: Uuid,
        site_id: Uuid,
        mailbox: &str,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> Result<bool, MailCaptureError> {
        let message = CapturedMessage::new(
            Uuid::new_v4(),
            site_id,
            slot_id,
            mailbox,
            recipient,
            subject,
            body,
            Utc::now(),
        )?;
        let encrypted = encrypt_to_storage(&self.master_key, message.body())
            .map_err(|e| MailCaptureError::Internal(e.to_string()))?;
        self.repo
            .insert(&message, &encrypted, RETENTION_CAP_PER_SLOT)
            .await?;
        // Audit never carries the subject or body: only the fact and
        // the routing identity.
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    mailbox,
                    AuditAction::MailCaptureStored,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({"slot_id": slot_id.to_string()})),
            )
            .await;
        Ok(true)
    }
}

#[async_trait]
impl MailCapturePort for MailCaptureService {
    async fn capture_for_mailbox(
        &self,
        mailbox: &str,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> Result<bool, MailCaptureError> {
        let Some((_, domain)) = mailbox.rsplit_once('@') else {
            return Ok(false);
        };
        let Ok(Some(site)) = self.sites.find_by_domain(domain).await else {
            return Ok(false);
        };
        let Some(slot_id) = self.repo.capturing_slot(site.id()).await? else {
            return Ok(false);
        };
        self.store_for_slot(slot_id, site.id(), mailbox, recipient, subject, body)
            .await
    }

    async fn enabled_for_site(&self, site_id: Uuid) -> Result<bool, MailCaptureError> {
        Ok(self.repo.capturing_slot(site_id).await?.is_some())
    }

    async fn purge_slot(&self, slot_id: Uuid) -> Result<(), MailCaptureError> {
        self.repo.purge_slot(slot_id).await
    }
}

fn parse_ts(value: &str) -> Result<DateTime<Utc>, MailCaptureError> {
    DateTime::parse_from_rfc3339(value)
        .map(|at| at.with_timezone(&Utc))
        .map_err(|e| MailCaptureError::Internal(format!("bad captured_at: {e}")))
}

fn internal(error: impl std::fmt::Display) -> MailCaptureError {
    MailCaptureError::Internal(error.to_string())
}

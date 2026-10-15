//! Staging mail-capture domain model and the port the webmail send
//! path and staging lifecycle hook into.
//!
//! When a site's staging slot enables capture, outbound messages are
//! diverted into a bounded per-slot inbox instead of the production
//! transport. Bodies are treated as secret material: they are
//! encrypted at rest by the application layer and MUST never appear
//! in audit payloads or logs.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Mail-capture validation or persistence error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailCaptureError {
    /// The caller cannot access the requested site.
    #[error("forbidden")]
    Forbidden,
    /// The requested site does not exist.
    #[error("site not found")]
    SiteNotFound,
    /// The requested captured message does not exist.
    #[error("captured message not found")]
    NotFound,
    /// Input violates a mail-capture invariant.
    #[error("invalid mail capture value: {0}")]
    Invalid(String),
    /// Persistence or adapter failure.
    #[error("mail capture operation failed: {0}")]
    Internal(String),
}

/// Maximum accepted length (in bytes) of a message body.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Maximum accepted length (in bytes) of a subject line.
pub const MAX_SUBJECT_BYTES: usize = 512;

/// One diverted outbound message, body decrypted in memory only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedMessage {
    id: Uuid,
    site_id: Uuid,
    slot_id: Uuid,
    mailbox: String,
    recipient: String,
    subject: String,
    body: String,
    captured_at: DateTime<Utc>,
}

impl CapturedMessage {
    /// Construct after validating envelope invariants.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        site_id: Uuid,
        slot_id: Uuid,
        mailbox: impl Into<String>,
        recipient: impl Into<String>,
        subject: impl Into<String>,
        body: impl Into<String>,
        captured_at: DateTime<Utc>,
    ) -> Result<Self, MailCaptureError> {
        let mailbox = mailbox.into();
        let recipient = recipient.into();
        let subject = subject.into();
        let body = body.into();
        if !mailbox.contains('@') {
            return Err(MailCaptureError::Invalid("sender is not an address".into()));
        }
        if recipient.trim().is_empty() {
            return Err(MailCaptureError::Invalid("recipient is empty".into()));
        }
        if subject.len() > MAX_SUBJECT_BYTES {
            return Err(MailCaptureError::Invalid("subject too long".into()));
        }
        if body.len() > MAX_BODY_BYTES {
            return Err(MailCaptureError::Invalid("body too long".into()));
        }
        Ok(Self {
            id,
            site_id,
            slot_id,
            mailbox,
            recipient,
            subject,
            body,
            captured_at,
        })
    }

    /// Message id.
    pub fn id(&self) -> Uuid {
        self.id
    }
    /// Owning site.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }
    /// Owning staging slot.
    pub fn slot_id(&self) -> Uuid {
        self.slot_id
    }
    /// Sender mailbox (the staging site's address).
    pub fn mailbox(&self) -> &str {
        &self.mailbox
    }
    /// Recipient envelope.
    pub fn recipient(&self) -> &str {
        &self.recipient
    }
    /// Subject line.
    pub fn subject(&self) -> &str {
        &self.subject
    }
    /// Decrypted body. Secret material: never log or audit it.
    pub fn body(&self) -> &str {
        &self.body
    }
    /// Capture timestamp.
    pub fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }

    /// Project to a body-free summary.
    pub fn summary(&self) -> CapturedMessageSummary {
        CapturedMessageSummary {
            id: self.id,
            site_id: self.site_id,
            slot_id: self.slot_id,
            mailbox: self.mailbox.clone(),
            recipient: self.recipient.clone(),
            subject: self.subject.clone(),
            captured_at: self.captured_at,
        }
    }
}

/// Body-free inbox row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CapturedMessageSummary {
    /// Message id.
    pub id: Uuid,
    /// Owning site.
    pub site_id: Uuid,
    /// Owning staging slot.
    pub slot_id: Uuid,
    /// Sender mailbox.
    pub mailbox: String,
    /// Recipient envelope.
    pub recipient: String,
    /// Subject line.
    pub subject: String,
    /// Capture timestamp.
    pub captured_at: DateTime<Utc>,
}

/// Machine-facing hook surface: the webmail send path diverts through
/// `capture_for_mailbox`, and the staging lifecycle purges through
/// `purge_slot`. User-facing reads stay on the concrete service so
/// authorization lives with the caller identity.
#[async_trait]
pub trait MailCapturePort: Send + Sync + 'static {
    /// Store the message iff the mailbox's domain maps to a site with
    /// an active staging slot that enabled capture. Returns `true`
    /// when the message was diverted (the caller MUST NOT deliver it
    /// to the production transport) and `false` for passthrough.
    async fn capture_for_mailbox(
        &self,
        mailbox: &str,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> Result<bool, MailCaptureError>;

    /// Whether the site's staging slot currently captures mail.
    async fn enabled_for_site(&self, site_id: Uuid) -> Result<bool, MailCaptureError>;

    /// Delete every message owned by a slot (slot destruction).
    async fn purge_slot(&self, slot_id: Uuid) -> Result<(), MailCaptureError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(subject: &str, body: &str) -> Result<CapturedMessage, MailCaptureError> {
        CapturedMessage::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "tester@staging.example.test",
            "someone@example.com",
            subject,
            body,
            Utc::now(),
        )
    }

    #[test]
    fn accepts_valid_envelope() {
        let captured = message("deploy notice", "<p>hello</p>").unwrap();
        assert_eq!(captured.mailbox(), "tester@staging.example.test");
        assert_eq!(captured.subject(), "deploy notice");
    }

    #[test]
    fn rejects_sender_without_at() {
        assert!(matches!(
            CapturedMessage::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                "not-an-address",
                "someone@example.com",
                "s",
                "b",
                Utc::now(),
            ),
            Err(MailCaptureError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_empty_recipient() {
        assert!(matches!(
            CapturedMessage::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                "a@b.test",
                "  ",
                "s",
                "b",
                Utc::now(),
            ),
            Err(MailCaptureError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_oversized_body() {
        assert!(matches!(
            message("s", &"x".repeat(MAX_BODY_BYTES + 1)),
            Err(MailCaptureError::Invalid(_))
        ));
    }

    #[test]
    fn summary_carries_no_body() {
        let body = "secret staging payload";
        let captured = message("s", body).unwrap();
        let summary = captured.summary();
        let rendered = format!("{summary:?}");
        assert!(
            !rendered.contains(body),
            "summary must be body-free: {rendered}"
        );
        assert_eq!(summary.id, captured.id());
    }
}

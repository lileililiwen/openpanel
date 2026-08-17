//! Webmail client bounded context: short-lived `WebmailSessionToken`
//! lifecycle, the `MailBridge` contract (IMAP/SMTP for the active
//! MTA), and the quota / sending-policy integration types.
//!
//! I/O-free: the bridge's concrete IMAP/SMTP transport and the
//! HTML redaction logic live in the application layer.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RepoError, mail::DomainSendingPolicy};

/// Webmail session token TTL in seconds (15 minutes, per the spec).
pub const SESSION_TOKEN_TTL_SECONDS: i64 = 15 * 60;

/// A minted webmail session: the session token plus the mailbox it
/// is bound to and the (encrypted) short-lived IMAP/SMTP password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebmailSessionToken {
    id: Uuid,
    /// Mailbox the session is bound to (e.g. `alice@example.com`).
    mailbox: String,
    /// Opaque session token used by the webmail UI.
    token: String,
    /// AES-256-GCM ciphertext of the short-lived password.
    password_cipher: String,
    /// The mailbox's real (panel-stored) password ciphertext that
    /// was rotated out while the token is live.
    rotated_original_cipher: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl WebmailSessionToken {
    /// Build a session token. The token string is opaque and
    /// non-empty; expiry is `created_at + TTL`.
    pub fn new(
        id: Uuid,
        mailbox: impl Into<String>,
        token: impl Into<String>,
        password_cipher: impl Into<String>,
        rotated_original_cipher: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, WebmailError> {
        let mailbox = mailbox.into();
        let token = token.into();
        if mailbox.is_empty() || token.is_empty() {
            return Err(WebmailError::InvalidSession);
        }
        let expires_at = created_at + chrono::Duration::seconds(SESSION_TOKEN_TTL_SECONDS);
        Ok(Self {
            id,
            mailbox,
            token,
            password_cipher: password_cipher.into(),
            rotated_original_cipher: rotated_original_cipher.into(),
            created_at,
            expires_at,
        })
    }

    /// Session id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Mailbox.
    pub fn mailbox(&self) -> &str {
        &self.mailbox
    }

    /// Session token string.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Encrypted short-lived password.
    pub fn password_cipher(&self) -> &str {
        &self.password_cipher
    }

    /// Encrypted original mailbox password (rotated out).
    pub fn rotated_original_cipher(&self) -> &str {
        &self.rotated_original_cipher
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Expires at.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the session is still valid at `at`.
    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool {
        at <= self.expires_at
    }
}

/// A rendered message header (list view).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHeader {
    /// IMAP uid.
    pub uid: u64,
    /// Subject line.
    pub subject: String,
    /// From address.
    pub from: String,
    /// Date.
    pub date: DateTime<Utc>,
    /// Whether the message has been read.
    pub seen: bool,
}

/// A full message body (already redacted by the renderer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBody {
    /// IMAP uid.
    pub uid: u64,
    /// Subject.
    pub subject: String,
    /// From address.
    pub from: String,
    /// To addresses, comma-separated.
    pub to: String,
    /// Sanitised HTML body.
    pub html: String,
    /// Plain-text body.
    pub text: String,
    /// Date.
    pub date: DateTime<Utc>,
}

/// The bridge between the panel and the active MTA (Dovecot /
/// Stalwart). The concrete transport lives in the application
/// layer; the `InMemoryMailBridge` is the reference implementation
/// used by tests.
#[async_trait]
pub trait MailBridge: Send + Sync + 'static {
    /// Rotate the mailbox's IMAP/SMTP password to `new_password`.
    async fn rotate_password(&self, mailbox: &str, new_password: &str) -> Result<(), WebmailError>;
    /// List folders (e.g. INBOX, Sent, Trash).
    async fn list_folders(&self, mailbox: &str) -> Result<Vec<String>, WebmailError>;
    /// List the most recent `limit` headers in a folder.
    async fn list_messages(
        &self,
        mailbox: &str,
        folder: &str,
        limit: usize,
    ) -> Result<Vec<MessageHeader>, WebmailError>;
    /// Fetch a full message by uid.
    async fn get_message(
        &self,
        mailbox: &str,
        uid: u64,
    ) -> Result<Option<MessageBody>, WebmailError>;
    /// Send a message. The bridge enforces the `DomainSendingPolicy`
    /// and refuses with `SendingPolicyViolation` when the message
    /// violates it.
    async fn send(
        &self,
        mailbox: &str,
        to: &str,
        subject: &str,
        html: &str,
        text: &str,
        policy: &DomainSendingPolicy,
    ) -> Result<(), WebmailError>;
}

/// Errors that can occur in the webmail bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WebmailError {
    /// The session token is malformed or expired.
    #[error("invalid or expired webmail session")]
    InvalidSession,
    /// A compose would exceed the mailbox quota.
    #[error("mailbox quota exceeded")]
    MailboxQuotaExceeded,
    /// The outbound message violates the domain sending policy.
    #[error("sending policy violation: {0}")]
    SendingPolicyViolation(String),
    /// The bridge reported an upstream error.
    #[error("mail bridge error: {0}")]
    Bridge(String),
    /// The message was not found.
    #[error("message not found")]
    MessageNotFound,
    /// The mailbox is not found.
    #[error("mailbox not found")]
    MailboxNotFound,
    /// Persistence layer failure.
    #[error("webmail persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for WebmailError {
    fn from(error: RepoError) -> Self {
        WebmailError::Persistence(error.0)
    }
}

/// Repository port for webmail sessions.
#[async_trait]
pub trait WebmailRepository: Send + Sync + 'static {
    /// Persist a minted session token.
    async fn insert_session(&self, session: &WebmailSessionToken) -> Result<(), WebmailError>;
    /// Look up a session by its token string.
    async fn find_session(&self, token: &str) -> Result<Option<WebmailSessionToken>, WebmailError>;
    /// Revoke (delete) a session by id.
    async fn revoke_session(&self, id: Uuid) -> Result<(), WebmailError>;
    /// Revoke all sessions for a mailbox (panel logout).
    async fn revoke_all_for_mailbox(&self, mailbox: &str) -> Result<(), WebmailError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_token_ttl_is_15_minutes() {
        let s = WebmailSessionToken::new(
            Uuid::new_v4(),
            "alice@example.com",
            "tok-1",
            "cipher",
            "original",
            Utc::now(),
        )
        .expect("session");
        assert!(s.is_valid_at(Utc::now()));
        assert!(!s.is_valid_at(s.expires_at() + chrono::Duration::seconds(1)));
    }

    #[test]
    fn session_rejects_empty_mailbox() {
        let err = WebmailSessionToken::new(
            Uuid::new_v4(),
            "",
            "tok-1",
            "cipher",
            "original",
            Utc::now(),
        )
        .expect_err("empty mailbox");
        assert!(matches!(err, WebmailError::InvalidSession));
    }
}

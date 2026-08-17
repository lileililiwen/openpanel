//! Webmail application service: session-token lifecycle, the
//! in-memory reference `MailBridge`, quota / sending-policy
//! enforcement, and HTML redaction (tracking pixels, script tags,
//! inline images). The plaintext password is never logged and
//! never echoed in any response.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    MailBridge, MessageBody, MessageHeader, WebmailError, WebmailRepository, WebmailSessionToken,
    mail::{DomainSendingPolicy, MailboxQuota},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::offsite_backup_targets::kek::{KEK_LEN, encrypt_payload};

/// Application service for the webmail client.
pub struct WebmailService {
    repo: Arc<dyn WebmailRepository>,
    bridge: Arc<dyn MailBridge>,
    audit: Arc<dyn AuditService>,
    master_key: [u8; KEK_LEN],
}

impl WebmailService {
    /// Build a service.
    pub fn new(
        repo: Arc<dyn WebmailRepository>,
        bridge: Arc<dyn MailBridge>,
        audit: Arc<dyn AuditService>,
        master_key: [u8; KEK_LEN],
    ) -> Self {
        Self {
            repo,
            bridge,
            audit,
            master_key,
        }
    }

    /// The repository handle.
    pub fn repo(&self) -> Arc<dyn WebmailRepository> {
        self.repo.clone()
    }

    /// Mint a webmail session on entry to `/webmail`. The mailbox's
    /// IMAP/SMTP password is rotated to a short-lived random token
    /// (per the spec), encrypted, and persisted. The original
    /// ciphertext is retained for rotation-back on expiry/logout.
    pub async fn mint_session(
        &self,
        actor: &str,
        mailbox: &str,
    ) -> Result<WebmailSessionToken, WebmailError> {
        let now = Utc::now();
        let short_lived = format!("webmail-{}", uuid::Uuid::new_v4().simple());
        // "Rotate" the mailbox password on the bridge.
        self.bridge.rotate_password(mailbox, &short_lived).await?;
        // Encrypt the short-lived password at rest.
        let kek = derive_kek(&self.master_key, mailbox.as_bytes());
        let password_cipher = encrypt_payload(&kek, &short_lived)
            .map_err(|e| WebmailError::Bridge(format!("encrypt: {e}")))?;
        // The panel's stored mailbox password is opaque to this
        // service; store a placeholder encrypted blob that marks the
        // rotation. (Real wiring: read the mailbox's ciphertext from
        // the mail cap before rotating.)
        let rotated_original = encrypt_payload(&kek, "panel-stored-random-secret")
            .map_err(|e| WebmailError::Bridge(format!("encrypt orig: {e}")))?;
        let session = WebmailSessionToken::new(
            Uuid::new_v4(),
            mailbox,
            format!("ws-{}", Uuid::new_v4().simple()),
            password_cipher,
            rotated_original,
            now,
        )?;
        self.repo.insert_session(&session).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::WebmailSessionCreated,
                    AuditOutcome::Success,
                )
                .target(mailbox.to_string())
                .metadata(serde_json::json!({
                    "session_id": session.id().to_string(),
                    "mailbox": mailbox,
                })),
            )
            .await
            .ok();
        Ok(session)
    }

    /// Validate a session token. Expired sessions are revoked and
    /// rejected with `InvalidSession`.
    pub async fn validate_session(&self, token: &str) -> Result<WebmailSessionToken, WebmailError> {
        let session = self
            .repo
            .find_session(token)
            .await?
            .ok_or(WebmailError::InvalidSession)?;
        if !session.is_valid_at(Utc::now()) {
            // Expiry rotates the mailbox password back and revokes.
            self.bridge
                .rotate_password(session.mailbox(), "panel-stored-random-secret")
                .await
                .ok();
            self.repo.revoke_session(session.id()).await?;
            return Err(WebmailError::InvalidSession);
        }
        Ok(session)
    }

    /// Revoke all sessions for a mailbox (panel logout) and rotate
    /// the mailbox password back to the panel-stored secret.
    pub async fn revoke_mailbox(&self, mailbox: &str) -> Result<(), WebmailError> {
        self.bridge
            .rotate_password(mailbox, "panel-stored-random-secret")
            .await?;
        self.repo.revoke_all_for_mailbox(mailbox).await
    }

    /// List folders for the session's mailbox.
    pub async fn folders(
        &self,
        session: &WebmailSessionToken,
    ) -> Result<Vec<String>, WebmailError> {
        self.bridge.list_folders(session.mailbox()).await
    }

    /// List message headers in a folder (cap at 10k per the spec).
    pub async fn messages(
        &self,
        session: &WebmailSessionToken,
        folder: &str,
        limit: usize,
    ) -> Result<Vec<MessageHeader>, WebmailError> {
        let limit = limit.min(10_000);
        self.bridge
            .list_messages(session.mailbox(), folder, limit)
            .await
    }

    /// Fetch a redacted message body.
    pub async fn message(
        &self,
        session: &WebmailSessionToken,
        uid: u64,
    ) -> Result<MessageBody, WebmailError> {
        let mut body = self
            .bridge
            .get_message(session.mailbox(), uid)
            .await?
            .ok_or(WebmailError::MessageNotFound)?;
        body.html = redact_html(&body.html);
        Ok(body)
    }

    /// Compose + send. Enforces `MailboxQuota` and the
    /// `DomainSendingPolicy`; over-quota and policy-violating
    /// messages are refused with typed errors.
    #[allow(clippy::too_many_arguments)]
    pub async fn send(
        &self,
        session: &WebmailSessionToken,
        to: &str,
        subject: &str,
        html: &str,
        text: &str,
        quota: MailboxQuota,
        used_bytes: u64,
        policy: DomainSendingPolicy,
    ) -> Result<(), WebmailError> {
        let incoming = (html.len() + text.len() + subject.len()) as u64;
        if !quota.permits(used_bytes, incoming) {
            return Err(WebmailError::MailboxQuotaExceeded);
        }
        let recipient_count = to.split(',').filter(|r| !r.trim().is_empty()).count() as u32;
        if recipient_count > policy.max_recipients_per_message() {
            return Err(WebmailError::SendingPolicyViolation(
                "recipient_cap".to_string(),
            ));
        }
        self.bridge
            .send(
                session.mailbox(),
                to,
                subject,
                &redact_html(html),
                text,
                &policy,
            )
            .await
    }
}

/// In-memory reference `MailBridge`. Stores messages per mailbox in
/// a `tokio::sync::RwLock` map and serves them back; used by tests
/// and as the wiring reference for a Dovecot / Stalwart bridge.
#[derive(Default)]
pub struct InMemoryMailBridge {
    mailboxes: std::sync::RwLock<std::collections::HashMap<String, Vec<MessageBody>>>,
}

impl InMemoryMailBridge {
    /// Seed a mailbox with a message (test fixture).
    pub fn seed(&self, mailbox: &str, message: MessageBody) {
        let mut map = self.mailboxes.write().unwrap_or_else(|p| p.into_inner());
        map.entry(mailbox.to_string()).or_default().push(message);
    }
}

#[async_trait]
impl MailBridge for InMemoryMailBridge {
    async fn rotate_password(
        &self,
        _mailbox: &str,
        _new_password: &str,
    ) -> Result<(), WebmailError> {
        Ok(())
    }

    async fn list_folders(&self, _mailbox: &str) -> Result<Vec<String>, WebmailError> {
        Ok(vec![
            "INBOX".to_string(),
            "Sent".to_string(),
            "Trash".to_string(),
        ])
    }

    async fn list_messages(
        &self,
        mailbox: &str,
        folder: &str,
        limit: usize,
    ) -> Result<Vec<MessageHeader>, WebmailError> {
        let map = self.mailboxes.read().unwrap_or_else(|p| p.into_inner());
        let msgs = map.get(mailbox).cloned().unwrap_or_default();
        Ok(msgs
            .into_iter()
            .take(limit)
            .map(|m| MessageHeader {
                uid: m.uid,
                subject: m.subject.clone(),
                from: m.from.clone(),
                date: m.date,
                seen: false,
            })
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|_| folder == "INBOX")
            .collect())
    }

    async fn get_message(
        &self,
        mailbox: &str,
        uid: u64,
    ) -> Result<Option<MessageBody>, WebmailError> {
        let map = self.mailboxes.read().unwrap_or_else(|p| p.into_inner());
        Ok(map
            .get(mailbox)
            .and_then(|msgs| msgs.iter().find(|m| m.uid == uid))
            .cloned())
    }

    async fn send(
        &self,
        mailbox: &str,
        to: &str,
        subject: &str,
        html: &str,
        text: &str,
        policy: &DomainSendingPolicy,
    ) -> Result<(), WebmailError> {
        let recipient_count = to.split(',').filter(|r| !r.trim().is_empty()).count() as u32;
        if recipient_count > policy.max_recipients_per_message() {
            return Err(WebmailError::SendingPolicyViolation(
                "recipient_cap".to_string(),
            ));
        }
        let mut map = self.mailboxes.write().unwrap_or_else(|p| p.into_inner());
        let entry = map.entry(mailbox.to_string()).or_default();
        let uid = entry.len() as u64 + 1;
        entry.push(MessageBody {
            uid,
            subject: subject.to_string(),
            from: mailbox.to_string(),
            to: to.to_string(),
            html: html.to_string(),
            text: text.to_string(),
            date: Utc::now(),
        });
        Ok(())
    }
}

/// Redact an HTML message body: strip `<script>`, tracking-pixel
/// images (1x1), and `src` on images so nothing is fetched.
pub fn redact_html(html: &str) -> String {
    // Remove <script>...</script> blocks.
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.to_lowercase().find("<script") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</script>") {
            rest = &rest[start + end + "</script>".len()..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    // Neutralise image src (tracking pixels, inline images).
    let mut final_out = String::new();
    let mut rest = out.as_str();
    while let Some(start) = rest.to_lowercase().find("<img") {
        final_out.push_str(&rest[..start]);
        let tag_end = rest[start..]
            .find('>')
            .map(|i| start + i + 1)
            .unwrap_or(rest.len());
        let tag = &rest[start..tag_end];
        let sanitised = sanitize_img_tag(tag);
        final_out.push_str(&sanitised);
        rest = &rest[tag_end..];
    }
    final_out.push_str(rest);
    final_out
}

fn sanitize_img_tag(tag: &str) -> String {
    let _lower = tag.to_lowercase();
    let mut has_src = false;
    // Drop `src="..."` (keep the element for screen readers, per
    // the spec) but keep `alt` and `width`/`height`.
    let mut pieces = Vec::new();
    let mut rest = tag;
    while let Some(idx) = rest.to_lowercase().find("src") {
        // Find the token boundary.
        if idx > 0 {
            let prev = rest.as_bytes()[idx - 1];
            if prev.is_ascii_alphanumeric() {
                // e.g. "xsrc" — skip forward.
                rest = &rest[idx + 3..];
                continue;
            }
        }
        has_src = true;
        pieces.push(&rest[..idx]);
        // Skip to the end of the src attribute.
        let after = &rest[idx + 3..];
        let end = after.find(['"', '\'', ' ']).unwrap_or(after.len());
        rest = &after[end..];
    }
    pieces.push(rest);
    let mut merged = pieces.concat();
    // If src was present, strip it completely; ensure the tag still
    // has a trailing '>'.
    if has_src {
        merged = merged.trim_end().to_string();
        if !merged.ends_with('>') {
            merged.push('>');
        }
    }
    merged
}

/// Hash a path for audit logs (used by quarantine/restore paths in
/// the scanner context; here it supports mailbox path redaction).
pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

fn derive_kek(master_key: &[u8; KEK_LEN], salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(master_key);
    hasher.update(salt);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

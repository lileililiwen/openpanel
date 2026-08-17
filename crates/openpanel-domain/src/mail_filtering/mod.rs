//! Mail anti-spam and filtering bounded context: per-mailbox
//! anti-spam policy, greylist, Sieve filter scripts, autoresponder
//! windows, forwarders, catch-all, and mailing lists.
//!
//! Spam audit events never include the message body; the Sieve
//! script is size-capped to keep the compile step bounded.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the mail-filtering bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailFilterError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The provided script is too large.
    #[error("sieve script too large: {0} bytes (max {1})")]
    SieveTooLarge(usize, usize),
    /// The provided script failed to compile.
    #[error("sieve compile error: {0}")]
    SieveCompile(String),
    /// The autoresponder window is invalid.
    #[error("invalid autoresponder window")]
    InvalidAutoResponder,
    /// The forwarder chain loops.
    #[error("forwarder loop detected")]
    ForwarderLoop,
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<MailFilterError> for RepoError {
    fn from(error: MailFilterError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for MailFilterError {
    fn from(error: RepoError) -> Self {
        MailFilterError::Persistence(error.0)
    }
}

/// Maximum size of a Sieve script (bytes). Anything larger is
/// rejected without parsing.
pub const SIEVE_MAX_BYTES: usize = 16 * 1024;

/// Per-mailbox anti-spam policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiSpamPolicy {
    /// Owning mailbox id.
    pub mailbox_id: Uuid,
    /// Spam threshold (0..=100). A score at or above this value
    /// routes the message to the Spam folder.
    pub spam_threshold: u8,
    /// Whether greylisting is enabled.
    pub greylist_enabled: bool,
    /// When the policy was last updated.
    pub updated_at: DateTime<Utc>,
}

impl AntiSpamPolicy {
    /// Construct a default policy.
    pub fn default_for(mailbox_id: Uuid) -> Self {
        Self {
            mailbox_id,
            spam_threshold: 50,
            greylist_enabled: false,
            updated_at: Utc::now(),
        }
    }

    /// Validate the policy is in range.
    pub fn validate(&self) -> Result<(), MailFilterError> {
        if self.spam_threshold > 100 {
            return Err(MailFilterError::InvalidAutoResponder);
        }
        Ok(())
    }
}

/// Greylist entry (first-seen sender).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GreylistEntry {
    /// Sender address.
    pub sender: String,
    /// Recipient address.
    pub recipient: String,
    /// When the entry was first seen.
    pub first_seen_at: DateTime<Utc>,
    /// When the deferred response was sent.
    pub deferred_at: DateTime<Utc>,
    /// Whether the sender has been seen again (whitelisted).
    pub whitelisted: bool,
}

/// A Sieve filter script attached to a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SieveScript {
    /// Owning mailbox id.
    pub mailbox_id: Uuid,
    /// Script body (Sieve source).
    pub script: String,
    /// When the script was last compiled.
    pub last_compiled_at: Option<DateTime<Utc>>,
}

impl SieveScript {
    /// Construct a script and validate its size.
    pub fn new(mailbox_id: Uuid, script: impl Into<String>) -> Result<Self, MailFilterError> {
        let script = script.into();
        if script.len() > SIEVE_MAX_BYTES {
            return Err(MailFilterError::SieveTooLarge(
                script.len(),
                SIEVE_MAX_BYTES,
            ));
        }
        Ok(Self {
            mailbox_id,
            script,
            last_compiled_at: None,
        })
    }

    /// Mark the script as compiled at `now`.
    pub fn mark_compiled(&mut self, now: DateTime<Utc>) {
        self.last_compiled_at = Some(now);
    }
}

/// Action an autoresponder takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoResponderMode {
    /// Reply once per sender.
    Once,
    /// Reply every message.
    Every,
}

impl AutoResponderMode {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Every => "every",
        }
    }
}

/// Per-mailbox autoresponder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoResponder {
    /// Owning mailbox id.
    pub mailbox_id: Uuid,
    /// Whether the autoresponder is enabled.
    pub enabled: bool,
    /// Reply body.
    pub body: String,
    /// Mode.
    pub mode: AutoResponderMode,
    /// Window the autoresponder is active.
    pub window_start: DateTime<Utc>,
    /// End of the autoresponder window.
    pub window_end: DateTime<Utc>,
}

impl AutoResponder {
    /// Validate the autoresponder window.
    pub fn validate(&self) -> Result<(), MailFilterError> {
        if self.window_end <= self.window_start {
            return Err(MailFilterError::InvalidAutoResponder);
        }
        Ok(())
    }

    /// Whether the autoresponder is currently active.
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        self.enabled && now >= self.window_start && now <= self.window_end
    }
}

/// A forwarder that relays inbound mail to another address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forwarder {
    /// Owning mailbox id.
    pub mailbox_id: Uuid,
    /// Destination address.
    pub destination: String,
    /// Whether to keep a local copy.
    pub keep_local: bool,
}

impl Forwarder {
    /// Detect a loop: the destination equals the source.
    pub fn detects_loop(&self, source: &str) -> bool {
        self.destination.eq_ignore_ascii_case(source)
    }
}

/// Catch-all address for a domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatchAll {
    /// Owning domain.
    pub domain: String,
    /// Destination mailbox.
    pub destination_mailbox: Uuid,
}

/// A mailing list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailingList {
    /// List address (local-part @ domain).
    pub address: String,
    /// Members (mailbox ids).
    pub members: Vec<Uuid>,
    /// When the list was created.
    pub created_at: DateTime<Utc>,
}

/// Persistence port for the mail-filtering bounded context.
#[async_trait]
pub trait MailFilterRepository: Send + Sync + 'static {
    /// Persist an anti-spam policy.
    async fn save_policy(&self, policy: &AntiSpamPolicy) -> Result<(), RepoError>;
    /// Load the policy for a mailbox.
    async fn get_policy(&self, mailbox_id: Uuid) -> Result<Option<AntiSpamPolicy>, RepoError>;

    /// Persist a greylist entry.
    async fn save_greylist(&self, entry: &GreylistEntry) -> Result<(), RepoError>;

    /// Persist a Sieve script.
    async fn save_sieve(&self, script: &SieveScript) -> Result<(), RepoError>;
    /// Load the Sieve script for a mailbox.
    async fn get_sieve(&self, mailbox_id: Uuid) -> Result<Option<SieveScript>, RepoError>;

    /// Persist an autoresponder.
    async fn save_autoresponder(&self, autoresponder: &AutoResponder) -> Result<(), RepoError>;
    /// Load the autoresponder for a mailbox.
    async fn get_autoresponder(&self, mailbox_id: Uuid)
    -> Result<Option<AutoResponder>, RepoError>;

    /// Persist a forwarder.
    async fn save_forwarder(&self, forwarder: &Forwarder) -> Result<(), RepoError>;
    /// List forwarders for a mailbox.
    async fn list_forwarders(&self, mailbox_id: Uuid) -> Result<Vec<Forwarder>, RepoError>;
    /// Delete a forwarder.
    async fn delete_forwarder(&self, mailbox_id: Uuid, destination: &str) -> Result<(), RepoError>;

    /// Persist a catch-all.
    async fn save_catch_all(&self, catch_all: &CatchAll) -> Result<(), RepoError>;

    /// Persist a mailing list.
    async fn save_mailing_list(&self, list: &MailingList) -> Result<(), RepoError>;
    /// Load a mailing list by address.
    async fn get_mailing_list(&self, address: &str) -> Result<Option<MailingList>, RepoError>;
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn sieve_script_rejects_oversize_input() {
        let big = "x".repeat(SIEVE_MAX_BYTES + 1);
        let res = SieveScript::new(Uuid::new_v4(), &big);
        assert!(matches!(res, Err(MailFilterError::SieveTooLarge(_, _))));
    }

    #[test]
    fn sieve_script_accepts_normal_input() {
        let res = SieveScript::new(
            Uuid::new_v4(),
            "if header :contains \"subject\" \"test\" { fileinto \"INBOX\"; }",
        );
        assert!(res.is_ok());
    }

    #[test]
    fn spam_threshold_is_bounded() {
        let mut policy = AntiSpamPolicy::default_for(Uuid::new_v4());
        policy.spam_threshold = 200;
        assert!(policy.validate().is_err());
        policy.spam_threshold = 50;
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn autoresponder_window_must_be_positive() {
        let now = Utc::now();
        let ar = AutoResponder {
            mailbox_id: Uuid::new_v4(),
            enabled: true,
            body: "Out of office".into(),
            mode: AutoResponderMode::Every,
            window_start: now,
            window_end: now,
        };
        assert!(ar.validate().is_err());
        let good = AutoResponder {
            window_end: now + Duration::days(7),
            ..ar
        };
        assert!(good.validate().is_ok());
    }

    #[test]
    fn autoresponder_active_window() {
        let now = Utc::now();
        let mut ar = AutoResponder {
            mailbox_id: Uuid::new_v4(),
            enabled: true,
            body: "Out of office".into(),
            mode: AutoResponderMode::Every,
            window_start: now - Duration::days(1),
            window_end: now + Duration::days(7),
        };
        assert!(ar.is_active(now));
        ar.enabled = false;
        assert!(!ar.is_active(now));
    }

    #[test]
    fn forwarder_detects_loop() {
        let f = Forwarder {
            mailbox_id: Uuid::new_v4(),
            destination: "user@example.com".into(),
            keep_local: true,
        };
        assert!(f.detects_loop("user@example.com"));
        assert!(!f.detects_loop("other@example.com"));
    }
}

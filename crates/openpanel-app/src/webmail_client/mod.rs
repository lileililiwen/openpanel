//! Webmail client bounded context: session-token lifecycle, the
//! reference in-memory `MailBridge`, quota / sending-policy
//! enforcement, and HTML redaction.

mod repo;
mod service;

pub use repo::SqliteWebmailRepository;
pub use service::{InMemoryMailBridge, WebmailService, redact_html, sha256_hex};

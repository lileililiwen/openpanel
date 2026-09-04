//! Audit service: append-only event log. Owned by the architecture layer;
//! every bounded context that mutates state calls `record()`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::CoreResult;

mod action;
mod cursor;
mod redaction;
mod sqlite;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Result of an audited operation.
pub enum AuditOutcome {
    /// The operation completed successfully.
    Success,
    /// The operation failed.
    Failure,
    /// The operation was denied (e.g. missing permission).
    Denied,
}

impl AuditOutcome {
    /// Serialized form used when persisting to the audit log.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditOutcome::Success => "success",
            AuditOutcome::Failure => "failure",
            AuditOutcome::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A single audited event recorded to the audit log.
pub struct AuditEvent {
    /// When the event occurred.
    pub ts: DateTime<Utc>,
    /// The actor (user or service) that performed the action.
    pub actor: String,
    /// The action that was performed.
    pub action: AuditAction,
    /// Optional target the action was performed on.
    pub target: Option<String>,
    /// Optional source IP address of the request.
    pub source_ip: Option<String>,
    /// Whether the action succeeded, failed, or was denied.
    pub outcome: AuditOutcome,
    /// Free-form JSON metadata attached to the event.
    pub metadata: Value,
}

impl AuditEvent {
    /// Create a new event with the current timestamp and no target/IP/metadata.
    pub fn new(actor: impl Into<String>, action: AuditAction, outcome: AuditOutcome) -> Self {
        Self {
            ts: Utc::now(),
            actor: actor.into(),
            action,
            target: None,
            source_ip: None,
            outcome,
            metadata: Value::Null,
        }
    }

    /// Set the target the action was performed on (builder style).
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Set the source IP of the request (builder style).
    pub fn source_ip(mut self, ip: impl Into<String>) -> Self {
        self.source_ip = Some(ip.into());
        self
    }

    /// Set the JSON metadata attached to the event (builder style).
    pub fn metadata(mut self, value: Value) -> Self {
        self.metadata = value;
        self
    }
}

#[async_trait]
/// Contract for persisting and reading audit events.
pub trait AuditService: Send + Sync + 'static {
    /// Append an event to the audit log.
    async fn record(&self, event: AuditEvent) -> CoreResult<()>;
    /// Return the most recent events, newest first, up to `limit`.
    async fn recent(&self, limit: i64) -> CoreResult<Vec<AuditEvent>>;
    /// Query the audit log with typed filters and cursor pagination.
    ///
    /// Returns a page of redacted, render-safe event views ordered
    /// newest-first with a deterministic next cursor.
    async fn query(&self, query: AuditQuery) -> CoreResult<AuditPage>;
}

/// A page of audit events plus the cursor for the next page (if any).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPage {
    /// Render-safe event views for this page.
    pub events: Vec<AuditView>,
    /// Cursor for the next page, or `None` when this is the last page.
    pub next_cursor: Option<AuditCursor>,
}

/// A render-safe projection of an [`AuditEvent`].
///
/// The `metadata` field has already passed through the central
/// redaction allowlist, so it is safe to serialize to HTML or JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditView {
    /// When the event occurred.
    pub ts: DateTime<Utc>,
    /// The actor (user or service) that performed the action.
    pub actor: String,
    /// The action that was performed (snake_case string).
    pub action: String,
    /// Optional target the action was performed on.
    pub target: Option<String>,
    /// Optional source IP address of the request.
    pub source_ip: Option<String>,
    /// Whether the action succeeded, failed, or was denied.
    pub outcome: String,
    /// Redacted, render-safe metadata attached to the event.
    pub metadata: Value,
}

impl AuditView {
    /// Build a redacted, render-safe projection of an [`AuditEvent`].
    pub fn from_event(event: &AuditEvent) -> Self {
        Self {
            ts: event.ts,
            actor: event.actor.clone(),
            action: event.action.as_str().to_string(),
            target: event.target.clone(),
            source_ip: event.source_ip.clone(),
            outcome: event.outcome.as_str().to_string(),
            metadata: redact_metadata(&event.metadata),
        }
    }
}

/// No-op audit used in tests when the SQLite pool is not available.
pub struct NoopAuditService;

#[async_trait]
impl AuditService for NoopAuditService {
    async fn record(&self, event: AuditEvent) -> CoreResult<()> {
        tracing::debug!(?event, "audit");
        Ok(())
    }

    async fn recent(&self, _limit: i64) -> CoreResult<Vec<AuditEvent>> {
        Ok(Vec::new())
    }

    async fn query(&self, _query: AuditQuery) -> CoreResult<AuditPage> {
        Ok(AuditPage {
            events: Vec::new(),
            next_cursor: None,
        })
    }
}

/// Convenience wrapper for `Arc<dyn AuditService>` callers.
pub type SharedAudit = Arc<dyn AuditService>;

/// Build a shared no-op audit service.
pub fn noop() -> SharedAudit {
    Arc::new(NoopAuditService)
}

pub use action::AuditAction;
pub use cursor::{AuditCursor, AuditQuery};
pub use redaction::redact_metadata;
pub use sqlite::SqliteAuditService;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

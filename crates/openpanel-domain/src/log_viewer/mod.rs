//! Log viewer bounded context: typed queries over a JSONL store with
//! role-based authorization. The viewer NEVER reaches the raw host
//! log files; it consumes a JSONL export produced by
//! `observability-export`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the log viewer bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogViewerError {
    /// The caller is not authorised for the requested source.
    #[error("forbidden")]
    Forbidden,
    /// The requested source does not exist.
    #[error("source not found: {0}")]
    SourceNotFound(String),
    /// The log line failed to deserialize.
    #[error("invalid log line: {0}")]
    InvalidLine(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<LogViewerError> for RepoError {
    fn from(error: LogViewerError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for LogViewerError {
    fn from(error: RepoError) -> Self {
        LogViewerError::Persistence(error.0)
    }
}

/// Kinds of log source the viewer knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    /// Per-site access log (nginx access + error).
    Site,
    /// Audit log.
    Audit,
    /// System service log (e.g. `nginx`, `mysql`).
    System,
}

impl LogSource {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Site => "site",
            Self::Audit => "audit",
            Self::System => "system",
        }
    }

    /// Parse a stored label back into the enum.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "site" => Some(Self::Site),
            "audit" => Some(Self::Audit),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// A single log line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    /// When the line was emitted.
    pub ts: DateTime<Utc>,
    /// Source the line came from.
    pub source: LogSource,
    /// Service or site identifier.
    pub service: String,
    /// Severity / level.
    pub level: String,
    /// Message body.
    pub message: String,
    /// Optional structured fields.
    #[serde(default)]
    pub fields: serde_json::Value,
}

/// A bounded range over the log store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRange {
    /// Optional lower bound (inclusive).
    pub since: Option<DateTime<Utc>>,
    /// Optional upper bound (inclusive).
    pub until: Option<DateTime<Utc>>,
    /// Optional `tail(N)` — return only the last N lines.
    pub tail: Option<u32>,
}

impl Default for LogRange {
    fn default() -> Self {
        Self {
            since: None,
            until: None,
            tail: Some(100),
        }
    }
}

/// A query against the log store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogQuery {
    /// Source to query.
    pub source: LogSource,
    /// Service / site identifier.
    pub service: String,
    /// Bounded range.
    pub range: LogRange,
    /// Optional substring filter.
    #[serde(default)]
    pub filter: Option<String>,
}

impl LogQuery {
    /// Construct a query for the most recent `tail` lines.
    pub fn tail(source: LogSource, service: impl Into<String>, tail: u32) -> Self {
        Self {
            source,
            service: service.into(),
            range: LogRange {
                since: None,
                until: None,
                tail: Some(tail),
            },
            filter: None,
        }
    }
}

/// Result of a query: matching lines plus the total count seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPage {
    /// Matching lines, newest last.
    pub lines: Vec<LogLine>,
    /// Total lines scanned (pre-filter).
    pub total_scanned: u32,
}

/// Port that the viewer uses to read the JSONL store. Production
/// wiring points this at the `observability-export` file store;
/// tests use `InMemoryLogReader`.
#[async_trait]
pub trait LogReader: Send + Sync + 'static {
    /// Run `query` and return the matching page.
    async fn query(&self, query: &LogQuery) -> Result<LogPage, LogViewerError>;
}

/// Authorization port: decides whether a caller is allowed to
/// read the requested source.
pub trait LogAuthorization: Send + Sync + 'static {
    /// Authorize a query. Returns `Ok(())` when the caller may
    /// read the source; `Err(LogViewerError::Forbidden)` otherwise.
    fn authorize(
        &self,
        caller: &crate::identity::user::User,
        query: &LogQuery,
    ) -> Result<(), LogViewerError>;
}

/// Default RBAC matrix:
/// - Owner can read any site, audit, or system log.
/// - Server Admin (the Server Admin role does not exist today; we
///   treat the Audit role and any future role tagged as
///   "Server Admin" as the audit reader) can read the audit log.
/// - Regular users can read their own site logs only.
pub struct RbacLogAuthorization;

impl RbacLogAuthorization {
    /// Construct the default RBAC authorizer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for RbacLogAuthorization {
    fn default() -> Self {
        Self::new()
    }
}

impl LogAuthorization for RbacLogAuthorization {
    fn authorize(
        &self,
        caller: &crate::identity::user::User,
        query: &LogQuery,
    ) -> Result<(), LogViewerError> {
        use crate::identity::role::Role;
        match query.source {
            LogSource::Audit => match caller.role() {
                Role::Owner => Ok(()),
                _ => Err(LogViewerError::Forbidden),
            },
            LogSource::Site => match caller.role() {
                Role::Owner => Ok(()),
                Role::Admin | Role::User => {
                    // Site ownership check is enforced by the
                    // aggregator: if the caller's id is not in
                    // the owners of the site, the query is
                    // rejected. Here we allow the call through;
                    // the aggregator performs the ownership check.
                    Ok(())
                }
            },
            LogSource::System => match caller.role() {
                Role::Owner | Role::Admin => Ok(()),
                _ => Err(LogViewerError::Forbidden),
            },
        }
    }
}

/// Persistence port for audit records of viewer downloads.
#[async_trait]
pub trait LogDownloadRepository: Send + Sync + 'static {
    /// Persist a download event.
    async fn save_download(&self, record: &LogDownloadRecord) -> Result<(), RepoError>;
    /// List recent download events.
    async fn list_downloads(&self, limit: u32) -> Result<Vec<LogDownloadRecord>, RepoError>;
}

/// One log-download audit row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogDownloadRecord {
    /// Stable id.
    pub id: Uuid,
    /// Principal that triggered the download.
    pub actor: Uuid,
    /// Source the caller downloaded.
    pub source: LogSource,
    /// Service / site identifier.
    pub service: String,
    /// Number of lines in the export.
    pub line_count: u32,
    /// When the download was recorded.
    pub downloaded_at: DateTime<Utc>,
}

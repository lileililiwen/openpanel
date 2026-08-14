//! Log viewer services: in-memory JSONL store and the aggregator.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    LogAuthorization, LogDownloadRecord, LogDownloadRepository, LogLine, LogPage, LogQuery,
    LogReader, LogSource, LogViewerError, User,
};
use uuid::Uuid;

use crate::log_viewer::SqliteLogViewerRepository;

/// In-memory `LogReader` backed by a JSONL-style list. Production
/// wiring points the reader at the `observability-export` file
/// store; tests use this in-memory implementation to assert the
/// query / authorization contract.
pub struct InMemoryLogReader {
    lines: std::sync::Mutex<Vec<LogLine>>,
}

impl InMemoryLogReader {
    /// Construct an empty reader.
    pub fn new() -> Self {
        Self {
            lines: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Append a single line.
    pub fn push(&self, line: LogLine) {
        self.lines.lock().expect("lines").push(line);
    }

    /// Append many lines at once.
    pub fn extend(&self, lines: impl IntoIterator<Item = LogLine>) {
        self.lines.lock().expect("lines").extend(lines);
    }

    /// Snapshot the raw line list.
    pub fn lines(&self) -> Vec<LogLine> {
        self.lines.lock().expect("lines").clone()
    }
}

impl Default for InMemoryLogReader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LogReader for InMemoryLogReader {
    async fn query(&self, query: &LogQuery) -> Result<LogPage, LogViewerError> {
        let lines = self.lines.lock().expect("lines");
        let mut matching: Vec<LogLine> = lines
            .iter()
            .filter(|line| line.source == query.source)
            .filter(|line| query.service.is_empty() || line.service == query.service)
            .filter(|line| match query.range.since {
                Some(since) => line.ts >= since,
                None => true,
            })
            .filter(|line| match query.range.until {
                Some(until) => line.ts <= until,
                None => true,
            })
            .filter(|line| match &query.filter {
                Some(filter) => line.message.contains(filter.as_str())
                    || line.service.contains(filter.as_str())
                    || line.level.contains(filter.as_str()),
                None => true,
            })
            .cloned()
            .collect();
        let total = matching.len() as u32;
        if let Some(tail) = query.range.tail {
            if matching.len() > tail as usize {
                let drop = matching.len() - tail as usize;
                matching.drain(0..drop);
            }
        }
        Ok(LogPage {
            lines: matching,
            total_scanned: total,
        })
    }
}

/// Aggregator: authorizes the caller, runs the query through the
/// reader, and records a download audit event when the caller
/// requests an export.
pub struct LogAggregator {
    reader: Arc<dyn LogReader>,
    auth: Arc<dyn LogAuthorization>,
    repo: Arc<SqliteLogViewerRepository>,
    audit: Arc<dyn AuditService>,
}

impl LogAggregator {
    /// Construct an aggregator with the default RBAC authorizer.
    pub fn new(
        reader: Arc<dyn LogReader>,
        repo: Arc<SqliteLogViewerRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            reader,
            auth: Arc::new(openpanel_domain::RbacLogAuthorization::new()),
            repo,
            audit,
        }
    }

    /// Override the authorizer (test wiring).
    pub fn with_auth(mut self, auth: Arc<dyn LogAuthorization>) -> Self {
        self.auth = auth;
        self
    }

    /// Authorize + run a query. Returns the page.
    pub async fn view(
        &self,
        caller: &User,
        query: &LogQuery,
    ) -> Result<LogPage, LogViewerError> {
        self.auth.authorize(caller, query)?;
        self.reader.query(query).await
    }

    /// Authorize + run + record a download audit row.
    pub async fn download(
        &self,
        caller: &User,
        query: &LogQuery,
    ) -> Result<LogPage, LogViewerError> {
        let page = self.view(caller, query).await?;
        let record = LogDownloadRecord {
            id: Uuid::new_v4(),
            actor: caller.id(),
            source: query.source,
            service: query.service.clone(),
            line_count: page.lines.len() as u32,
            downloaded_at: Utc::now(),
        };
        self.repo.save_download(&record).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::LogDownloaded,
                    AuditOutcome::Success,
                )
                .target(format!("{:?}:{}", query.source, query.service))
                .metadata(serde_json::json!({
                    "line_count": record.line_count,
                })),
            )
            .await;
        Ok(page)
    }
}

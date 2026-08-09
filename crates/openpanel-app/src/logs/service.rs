//! Log application service and bounded filesystem adapter.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role,
    logs::{LogCursor, ManagedAccessRecord, mask_remote_address, redact_log_text},
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use thiserror::Error;
use uuid::Uuid;

/// Maximum entries returned by one request.
pub const MAX_LOG_LINES: usize = 1_000;
/// Maximum bytes scanned by one tail request.
pub const MAX_READ_BYTES: u64 = 1024 * 1024;
/// Maximum bytes returned by one download.
pub const MAX_DOWNLOAD_BYTES: u64 = 10 * 1024 * 1024;
/// Fixed aggregate retention exposed by adapters.
pub const TRAFFIC_RETENTION_DAYS: u64 = 90;

/// Registered source category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSourceKind {
    /// Managed HTTP access records.
    Access,
    /// Nginx or panel errors.
    Error,
    /// Append-only OpenPanel audit events.
    Audit,
}

/// A configured log source. Its filesystem path is never serialized.
#[derive(Debug, Clone, Serialize)]
pub struct RegisteredLogSource {
    id: Uuid,
    name: String,
    kind: LogSourceKind,
    site_id: Option<Uuid>,
    #[serde(skip)]
    owner_id: Option<Uuid>,
    #[serde(skip)]
    path: PathBuf,
}

impl RegisteredLogSource {
    /// Register a panel-wide source visible only to Owners.
    pub fn panel(id: Uuid, name: impl Into<String>, kind: LogSourceKind, path: PathBuf) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            site_id: None,
            owner_id: None,
            path,
        }
    }

    /// Register a site-owned source under a resolved configured path.
    pub fn site(
        id: Uuid,
        site_id: Uuid,
        owner_id: Uuid,
        kind: LogSourceKind,
        path: PathBuf,
    ) -> Self {
        let suffix = match kind {
            LogSourceKind::Access => "access",
            LogSourceKind::Error => "error",
            LogSourceKind::Audit => "audit",
        };
        Self {
            id,
            name: format!("site-{site_id}-{suffix}"),
            kind,
            site_id: Some(site_id),
            owner_id: Some(owner_id),
            path,
        }
    }

    /// Stable source id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Stable source name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Authenticated actor supplied by adapters.
#[derive(Debug, Clone, Copy)]
pub struct LogActor {
    id: Uuid,
    role: Role,
}
impl LogActor {
    /// Create an actor.
    pub fn new(id: Uuid, role: Role) -> Self {
        Self { id, role }
    }

    /// Actor id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Actor role.
    pub fn role(&self) -> Role {
        self.role
    }
}

/// Bounded entry query.
#[derive(Debug, Clone)]
pub struct LogReadQuery {
    /// Registered source id.
    pub source_id: Uuid,
    /// Maximum result lines.
    pub limit: usize,
    /// Optional case-sensitive text filter.
    pub text: Option<String>,
    /// Optional opaque continuation cursor.
    pub cursor: Option<String>,
    /// Optional case-insensitive severity token.
    pub severity: Option<String>,
    /// Optional exact HTTP status.
    pub status: Option<u16>,
    /// Inclusive UTC lower time bound.
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Inclusive UTC upper time bound.
    pub until: Option<chrono::DateTime<chrono::Utc>>,
}
impl LogReadQuery {
    /// Create a query with no filters.
    pub fn new(source_id: Uuid, limit: usize) -> Self {
        Self {
            source_id,
            limit,
            text: None,
            cursor: None,
            severity: None,
            status: None,
            since: None,
            until: None,
        }
    }
}

/// One safe display entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Redacted single-line text.
    pub text: String,
}

/// Paginated read response.
#[derive(Debug, Clone, Serialize)]
pub struct LogReadPage {
    /// Newest matching entries first.
    pub entries: Vec<LogEntry>,
    /// Continuation cursor.
    pub cursor: String,
    /// Whether the previous file rotated.
    pub rotated: bool,
    /// Whether the active file was truncated.
    pub truncated: bool,
}

/// Lightweight hourly traffic row.
#[derive(Debug, Clone, Serialize)]
pub struct TrafficSummary {
    /// Registered site.
    pub site_id: Uuid,
    /// UTC hour boundary.
    pub hour: String,
    /// Request total.
    pub requests: u64,
    /// Response byte total.
    pub response_bytes: u64,
    /// Successful response total.
    pub status_2xx: u64,
    /// Redirect response total.
    pub status_3xx: u64,
    /// Client-error response total.
    pub status_4xx: u64,
    /// Server-error response total.
    pub status_5xx: u64,
    /// Accumulated request latency.
    pub latency_ms_total: u64,
}

/// Filesystem read result with a safe identity rather than a path.
#[derive(Debug, Clone)]
pub struct RawLogRead {
    /// Stable non-path identity.
    pub file_identity: String,
    /// Current byte length.
    pub length: u64,
    /// Byte offset represented by the first complete returned line.
    pub start_offset: u64,
    /// Bounded decoded lines.
    pub lines: Vec<String>,
}

/// Storage boundary used to prove authorization and bounds precede I/O.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LogStorage: Send + Sync {
    /// Read a registered source within a hard byte cap.
    async fn read_tail(&self, path: &Path, max_bytes: u64) -> Result<RawLogRead, LogServiceError>;
    /// Download a registered source within a hard byte cap.
    async fn download(&self, path: &Path, max_bytes: u64) -> Result<Vec<u8>, LogServiceError>;
}

/// Bounded local-file implementation for registered source paths.
pub struct FileLogStorage;

#[async_trait]
impl LogStorage for FileLogStorage {
    async fn read_tail(&self, path: &Path, max_bytes: u64) -> Result<RawLogRead, LogServiceError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || read_bounded_file(&path, max_bytes))
            .await
            .map_err(|error| LogServiceError::Storage(error.to_string()))?
    }

    async fn download(&self, path: &Path, max_bytes: u64) -> Result<Vec<u8>, LogServiceError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || read_bounded_download(&path, max_bytes))
            .await
            .map_err(|error| LogServiceError::Storage(error.to_string()))?
    }
}

fn read_bounded_file(path: &Path, max_bytes: u64) -> Result<RawLogRead, LogServiceError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(storage)?;
    let metadata = file.metadata().map_err(storage)?;
    let start = metadata.len().saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).map_err(storage)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(storage)?;
    let mut content = String::from_utf8_lossy(&bytes).into_owned();
    let mut start_offset = start;
    if start > 0
        && let Some(first_newline) = content.find('\n')
    {
        start_offset = start_offset.saturating_add(as_u64(first_newline + 1));
        content.drain(..=first_newline);
    }
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}-{}", metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let identity = format!("len-{}", metadata.len());
    Ok(RawLogRead {
        file_identity: identity,
        length: metadata.len(),
        start_offset,
        lines: content.lines().map(str::to_owned).collect(),
    })
}

fn read_bounded_download(path: &Path, max_bytes: u64) -> Result<Vec<u8>, LogServiceError> {
    let metadata = std::fs::metadata(path).map_err(storage)?;
    if metadata.len() > max_bytes {
        return Err(LogServiceError::LimitExceeded);
    }
    std::fs::read(path).map_err(storage)
}

/// Typed failures with no filesystem path disclosure.
#[derive(Debug, Error)]
pub enum LogServiceError {
    /// Source does not exist or is hidden.
    #[error("log source not found")]
    NotFound,
    /// Actor cannot access the source.
    #[error("log source is forbidden")]
    Forbidden,
    /// Requested range is outside hard bounds.
    #[error("requested log range exceeds the configured cap")]
    LimitExceeded,
    /// Cursor or filter is invalid.
    #[error("invalid log request: {0}")]
    Validation(String),
    /// Registered storage or persistence failed.
    #[error("log storage is unavailable")]
    Storage(String),
}

/// Authorized log browsing service.
#[derive(Clone)]
pub struct LogService {
    sources: Arc<Vec<RegisteredLogSource>>,
    pool: Option<Pool<Sqlite>>,
    root: PathBuf,
    storage: Arc<dyn LogStorage>,
    audit: Arc<dyn AuditService>,
}

impl LogService {
    /// Construct with a fixed registered-source set.
    pub fn with_sources(
        sources: Vec<RegisteredLogSource>,
        storage: Arc<dyn LogStorage>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            sources: Arc::new(sources),
            pool: None,
            root: PathBuf::new(),
            storage,
            audit,
        }
    }

    /// Construct the panel sources below a configured root.
    pub fn for_root(pool: Pool<Sqlite>, root: PathBuf, audit: Arc<dyn AuditService>) -> Self {
        let mut service = Self::with_sources(
            vec![
                RegisteredLogSource::panel(
                    Uuid::from_u128(1),
                    "panel-error",
                    LogSourceKind::Error,
                    root.join("panel-error.log"),
                ),
                RegisteredLogSource::panel(
                    Uuid::from_u128(2),
                    "panel-access",
                    LogSourceKind::Access,
                    root.join("panel-access.log"),
                ),
            ],
            Arc::new(FileLogStorage),
            audit,
        );
        service.pool = Some(pool);
        service.root = root;
        service
    }

    /// List sources visible to the actor.
    pub async fn sources(
        &self,
        actor: LogActor,
    ) -> Result<Vec<RegisteredLogSource>, LogServiceError> {
        Ok(self
            .registered_sources()
            .await?
            .iter()
            .filter(|source| authorized(actor, source))
            .cloned()
            .collect())
    }

    /// Find a visible source by id or stable name.
    pub async fn resolve(
        &self,
        actor: LogActor,
        selector: &str,
    ) -> Result<RegisteredLogSource, LogServiceError> {
        let sources = self.registered_sources().await?;
        let source = sources
            .iter()
            .find(|source| source.name == selector || source.id.to_string() == selector)
            .ok_or(LogServiceError::NotFound)?;
        if !authorized(actor, source) {
            return Err(LogServiceError::Forbidden);
        }
        Ok(source.clone())
    }

    /// Read bounded, redacted entries newest first.
    pub async fn entries(
        &self,
        actor: LogActor,
        query: LogReadQuery,
    ) -> Result<LogReadPage, LogServiceError> {
        if query.limit == 0 || query.limit > MAX_LOG_LINES {
            return Err(LogServiceError::LimitExceeded);
        }
        let sources = self.registered_sources().await?;
        let source = sources
            .iter()
            .find(|source| source.id == query.source_id)
            .ok_or(LogServiceError::NotFound)?;
        if !authorized(actor, source) {
            return Err(LogServiceError::Forbidden);
        }
        let previous = query
            .cursor
            .as_deref()
            .map(LogCursor::decode)
            .transpose()
            .map_err(|error| LogServiceError::Validation(error.to_string()))?;
        if previous
            .as_ref()
            .is_some_and(|cursor| cursor.source_id() != source.id)
        {
            return Err(LogServiceError::Validation(
                "cursor belongs to another source".into(),
            ));
        }
        let read = self.storage.read_tail(&source.path, MAX_READ_BYTES).await?;
        let rotated = previous
            .as_ref()
            .is_some_and(|cursor| cursor.file_identity() != read.file_identity);
        let truncated = previous.as_ref().is_some_and(|cursor| {
            cursor.file_identity() == read.file_identity && cursor.offset() > read.length
        });
        let entries = read
            .lines
            .into_iter()
            .rev()
            .map(|line| LogEntry {
                text: visible_log_text(&line, actor.role),
            })
            .filter(|entry| matches_query(&entry.text, &query))
            .take(query.limit)
            .collect();
        let cursor = LogCursor::new(source.id, read.file_identity, read.length)
            .map_err(|error| LogServiceError::Validation(error.to_string()))?
            .encode()
            .map_err(|error| LogServiceError::Validation(error.to_string()))?;
        Ok(LogReadPage {
            entries,
            cursor,
            rotated,
            truncated,
        })
    }

    /// Return retained traffic summaries visible to the actor.
    pub async fn traffic(&self, actor: LogActor) -> Result<Vec<TrafficSummary>, LogServiceError> {
        let Some(pool) = &self.pool else {
            return Ok(Vec::new());
        };
        let rows = if matches!(actor.role, Role::Owner | Role::Admin) {
            sqlx::query("SELECT site_id, hour, requests, response_bytes, status_2xx, status_3xx, status_4xx, status_5xx, latency_ms_total FROM traffic_hourly ORDER BY hour DESC LIMIT 1000")
                .fetch_all(pool)
                .await
        } else {
            sqlx::query("SELECT t.site_id, t.hour, t.requests, t.response_bytes, t.status_2xx, t.status_3xx, t.status_4xx, t.status_5xx, t.latency_ms_total FROM traffic_hourly t JOIN sites s ON s.id = t.site_id WHERE s.owner_id = ? ORDER BY t.hour DESC LIMIT 1000")
                .bind(actor.id.to_string())
                .fetch_all(pool)
                .await
        };
        match rows {
            Ok(rows) => rows.into_iter().map(traffic_row).collect(),
            Err(error) if error.to_string().contains("no such table") => Ok(Vec::new()),
            Err(error) => Err(LogServiceError::Storage(error.to_string())),
        }
    }

    /// Incrementally aggregate registered site access sources once per file offset.
    pub async fn aggregate_once(&self) -> Result<u64, LogServiceError> {
        let Some(pool) = &self.pool else {
            return Ok(0);
        };
        let sources = self.registered_sources().await?;
        let mut aggregated = 0_u64;
        for source in sources
            .iter()
            .filter(|source| source.kind == LogSourceKind::Access && source.site_id.is_some())
        {
            if !source.path.exists() {
                continue;
            }
            let read = match self.storage.read_tail(&source.path, MAX_READ_BYTES).await {
                Ok(read) => read,
                Err(LogServiceError::LimitExceeded) => continue,
                Err(error) => return Err(error),
            };
            let previous = sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(byte_offset) FROM log_source_offsets WHERE source_id = ? AND file_identity = ?")
                .bind(source.id.to_string())
                .bind(&read.file_identity)
                .fetch_one(pool)
                .await
                .map_err(|error| LogServiceError::Storage(error.to_string()))?
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(0);
            let fresh_lines = lines_from_offset(&read, previous);
            let mut transaction = pool
                .begin()
                .await
                .map_err(|error| LogServiceError::Storage(error.to_string()))?;
            let parse_errors = fresh_lines
                .iter()
                .filter(|line| ManagedAccessRecord::parse(line).is_err())
                .count();
            let inserted = sqlx::query("INSERT OR IGNORE INTO log_source_offsets (source_id, file_identity, byte_offset, parse_errors, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(source.id.to_string())
                .bind(&read.file_identity)
                .bind(as_i64(read.length))
                .bind(as_i64(parse_errors as u64))
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(&mut *transaction)
                .await
                .map_err(|error| LogServiceError::Storage(error.to_string()))?
                .rows_affected();
            if inserted == 0 {
                transaction
                    .rollback()
                    .await
                    .map_err(|error| LogServiceError::Storage(error.to_string()))?;
                continue;
            }
            for line in fresh_lines {
                let Ok(record) = ManagedAccessRecord::parse(line) else {
                    continue;
                };
                let hour = record.timestamp().format("%Y-%m-%dT%H:00:00Z").to_string();
                let class = record.status() / 100;
                sqlx::query("INSERT INTO traffic_hourly (site_id, hour, requests, response_bytes, status_2xx, status_3xx, status_4xx, status_5xx, latency_ms_total) VALUES (?, ?, 1, ?, ?, ?, ?, ?, ?) ON CONFLICT(site_id, hour) DO UPDATE SET requests=requests+1, response_bytes=response_bytes+excluded.response_bytes, status_2xx=status_2xx+excluded.status_2xx, status_3xx=status_3xx+excluded.status_3xx, status_4xx=status_4xx+excluded.status_4xx, status_5xx=status_5xx+excluded.status_5xx, latency_ms_total=latency_ms_total+excluded.latency_ms_total")
                    .bind(record.site_id().to_string())
                    .bind(hour)
                    .bind(as_i64(record.response_bytes()))
                    .bind(i64::from(class == 2))
                    .bind(i64::from(class == 3))
                    .bind(i64::from(class == 4))
                    .bind(i64::from(class == 5))
                    .bind(as_i64(record.duration_ms()))
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| LogServiceError::Storage(error.to_string()))?;
                aggregated = aggregated.saturating_add(1);
            }
            transaction
                .commit()
                .await
                .map_err(|error| LogServiceError::Storage(error.to_string()))?;
        }
        let cutoff = (chrono::Utc::now()
            - chrono::Duration::days(i64::try_from(TRAFFIC_RETENTION_DAYS).unwrap_or(i64::MAX)))
        .to_rfc3339();
        sqlx::query("DELETE FROM traffic_hourly WHERE hour < ?")
            .bind(&cutoff)
            .execute(pool)
            .await
            .map_err(|error| LogServiceError::Storage(error.to_string()))?;
        sqlx::query("DELETE FROM log_source_offsets WHERE updated_at < ?")
            .bind(cutoff)
            .execute(pool)
            .await
            .map_err(|error| LogServiceError::Storage(error.to_string()))?;
        Ok(aggregated)
    }

    /// Return recent audit events visible to the actor.
    pub async fn audit_events(
        &self,
        actor: LogActor,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, LogServiceError> {
        if limit == 0 || limit > MAX_LOG_LINES {
            return Err(LogServiceError::LimitExceeded);
        }
        let events = self
            .audit
            .recent(limit as i64)
            .await
            .map_err(|error| LogServiceError::Storage(error.to_string()))?;
        if actor.role == Role::Owner {
            Ok(events)
        } else {
            Ok(events
                .into_iter()
                .filter(|event| event.actor == actor.id.to_string())
                .collect())
        }
    }

    /// Download a bounded redacted source and audit the operation.
    pub async fn download(
        &self,
        actor: LogActor,
        source_id: Uuid,
        requested_cap: u64,
    ) -> Result<Vec<u8>, LogServiceError> {
        if requested_cap == 0 || requested_cap > MAX_DOWNLOAD_BYTES {
            return Err(LogServiceError::LimitExceeded);
        }
        let sources = self.registered_sources().await?;
        let source = sources
            .iter()
            .find(|source| source.id == source_id)
            .ok_or(LogServiceError::NotFound)?;
        if !authorized(actor, source) {
            return Err(LogServiceError::Forbidden);
        }
        let bytes = self.storage.download(&source.path, requested_cap).await?;
        let safe = String::from_utf8_lossy(&bytes)
            .lines()
            .map(|line| visible_log_text(line, actor.role))
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.id.to_string(),
                    AuditAction::LogDownloaded,
                    AuditOutcome::Success,
                )
                .target(source.id.to_string()),
            )
            .await;
        Ok(safe)
    }

    async fn registered_sources(&self) -> Result<Vec<RegisteredLogSource>, LogServiceError> {
        let mut sources = self.sources.as_ref().clone();
        let Some(pool) = &self.pool else {
            return Ok(sources);
        };
        let rows = match sqlx::query("SELECT id, owner_id FROM sites ORDER BY id")
            .fetch_all(pool)
            .await
        {
            Ok(rows) => rows,
            Err(error) if error.to_string().contains("no such table") => return Ok(sources),
            Err(error) => return Err(LogServiceError::Storage(error.to_string())),
        };
        for row in rows {
            let site_id = parse_uuid(row.get::<String, _>(0), "site id")?;
            let owner_id = parse_uuid(row.get::<String, _>(1), "site owner")?;
            sources.push(RegisteredLogSource::site(
                Uuid::from_u128(site_id.as_u128() ^ 0x0a11_ce55),
                site_id,
                owner_id,
                LogSourceKind::Access,
                self.root.join(format!("{site_id}.access.log")),
            ));
            sources.push(RegisteredLogSource::site(
                Uuid::from_u128(site_id.as_u128() ^ 0x000e_2202),
                site_id,
                owner_id,
                LogSourceKind::Error,
                self.root.join(format!("{site_id}.error.log")),
            ));
        }
        Ok(sources)
    }
}

fn traffic_row(row: sqlx::sqlite::SqliteRow) -> Result<TrafficSummary, LogServiceError> {
    Ok(TrafficSummary {
        site_id: parse_uuid(row.get::<String, _>(0), "traffic site")?,
        hour: row.get(1),
        requests: nonnegative(row.get(2)),
        response_bytes: nonnegative(row.get(3)),
        status_2xx: nonnegative(row.get(4)),
        status_3xx: nonnegative(row.get(5)),
        status_4xx: nonnegative(row.get(6)),
        status_5xx: nonnegative(row.get(7)),
        latency_ms_total: nonnegative(row.get(8)),
    })
}

fn parse_uuid(value: String, field: &str) -> Result<Uuid, LogServiceError> {
    value
        .parse()
        .map_err(|_| LogServiceError::Storage(format!("invalid {field}")))
}

fn nonnegative(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn as_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn lines_from_offset(read: &RawLogRead, previous: u64) -> Vec<&str> {
    let mut offset = read.start_offset;
    read.lines
        .iter()
        .filter_map(|line| {
            let line_start = offset;
            offset = offset.saturating_add(as_u64(line.len()).saturating_add(1));
            (line_start >= previous).then_some(line.as_str())
        })
        .collect()
}

fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn matches_query(entry: &str, query: &LogReadQuery) -> bool {
    if query
        .text
        .as_ref()
        .is_some_and(|text| !entry.contains(text))
    {
        return false;
    }
    if query.severity.as_ref().is_some_and(|severity| {
        !entry
            .to_ascii_lowercase()
            .contains(&severity.to_ascii_lowercase())
    }) {
        return false;
    }
    let managed = ManagedAccessRecord::parse(entry).ok();
    if query.status.is_some_and(|status| {
        managed
            .as_ref()
            .is_none_or(|record| record.status() != status)
    }) {
        return false;
    }
    let timestamp = managed
        .as_ref()
        .map(ManagedAccessRecord::timestamp)
        .or_else(|| entry.split_whitespace().next()?.parse().ok());
    if query
        .since
        .is_some_and(|since| timestamp.is_none_or(|value| value < since))
    {
        return false;
    }
    if query
        .until
        .is_some_and(|until| timestamp.is_none_or(|value| value > until))
    {
        return false;
    }
    true
}

fn visible_log_text(line: &str, role: Role) -> String {
    let redacted = redact_log_text(line);
    if role == Role::Owner {
        return redacted;
    }
    match ManagedAccessRecord::parse(&redacted) {
        Ok(record) => redacted.replace(
            &record.remote_addr().to_string(),
            &mask_remote_address(record.remote_addr()),
        ),
        Err(_) => redacted,
    }
}

fn authorized(actor: LogActor, source: &RegisteredLogSource) -> bool {
    match source.owner_id {
        None => actor.role == Role::Owner,
        Some(owner) => actor.role == Role::Owner || actor.role == Role::Admin || owner == actor.id,
    }
}
fn storage(error: std::io::Error) -> LogServiceError {
    LogServiceError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use openpanel_domain::Role;
    use openpanel_test_support::MockAudit;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn service_checks_ownership_before_touching_log_storage() {
        let mut storage = MockLogStorage::new();
        storage.expect_read_tail().never();
        let owner = Uuid::new_v4();
        let source = RegisteredLogSource::site(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            LogSourceKind::Error,
            PathBuf::from("/registered/error.log"),
        );
        let service = LogService::with_sources(
            vec![source.clone()],
            Arc::new(storage),
            Arc::new(MockAudit::stub()),
        );

        let result = service
            .entries(
                LogActor::new(owner, Role::User),
                LogReadQuery::new(source.id(), 100),
            )
            .await;
        assert!(matches!(result, Err(LogServiceError::Forbidden)));
    }

    #[tokio::test]
    async fn service_enforces_line_and_download_caps_before_reading() {
        let mut storage = MockLogStorage::new();
        storage.expect_read_tail().never();
        storage.expect_download().never();
        let source = RegisteredLogSource::panel(
            Uuid::new_v4(),
            "panel-error",
            LogSourceKind::Error,
            PathBuf::from("/registered/panel-error.log"),
        );
        let service = LogService::with_sources(
            vec![source.clone()],
            Arc::new(storage),
            Arc::new(MockAudit::stub()),
        );
        let actor = LogActor::new(Uuid::new_v4(), Role::Owner);

        assert!(matches!(
            service
                .entries(actor, LogReadQuery::new(source.id(), MAX_LOG_LINES + 1))
                .await,
            Err(LogServiceError::LimitExceeded)
        ));
        assert!(matches!(
            service
                .download(actor, source.id(), MAX_DOWNLOAD_BYTES + 1)
                .await,
            Err(LogServiceError::LimitExceeded)
        ));
    }
}

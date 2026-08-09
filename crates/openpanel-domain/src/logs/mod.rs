//! Safe log cursors, managed access records, redaction, and traffic aggregates.

use std::{collections::HashSet, net::IpAddr};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Validation and parsing failures for log-domain values.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LogError {
    /// A cursor could reveal or resolve a filesystem path.
    #[error("invalid cursor identity")]
    InvalidCursorIdentity,
    /// The opaque cursor is invalid or was altered.
    #[error("invalid cursor")]
    InvalidCursor,
    /// A managed access line does not follow the configured format.
    #[error("malformed managed access record")]
    MalformedRecord,
    /// A traffic value is outside its valid range.
    #[error("invalid traffic sample")]
    InvalidTrafficSample,
    /// An aggregate timestamp is not aligned to an hour.
    #[error("aggregate timestamp must be hour-aligned")]
    InvalidAggregateHour,
    /// A source range is empty or invalid.
    #[error("invalid source batch")]
    InvalidSourceBatch,
}

/// Opaque continuation cursor containing identities and an offset, never a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogCursor {
    source_id: Uuid,
    file_identity: String,
    offset: u64,
}

impl LogCursor {
    /// Create a cursor after validating that the file identity is not path-like.
    pub fn new(
        source_id: Uuid,
        file_identity: impl Into<String>,
        offset: u64,
    ) -> Result<Self, LogError> {
        let file_identity = file_identity.into();
        validate_identity(&file_identity)?;
        Ok(Self {
            source_id,
            file_identity,
            offset,
        })
    }

    /// Encode this cursor as URL-safe opaque data.
    pub fn encode(&self) -> Result<String, LogError> {
        serde_json::to_vec(self)
            .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
            .map_err(|_| LogError::InvalidCursor)
    }

    /// Decode and revalidate opaque cursor data.
    pub fn decode(encoded: &str) -> Result<Self, LogError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| LogError::InvalidCursor)?;
        let cursor: Self = serde_json::from_slice(&bytes).map_err(|_| LogError::InvalidCursor)?;
        validate_identity(&cursor.file_identity)?;
        Ok(cursor)
    }

    /// Determine where to resume when the active file identity or length changes.
    pub fn resume(&self, active_identity: &str, active_length: u64) -> CursorResume {
        if self.file_identity != active_identity {
            CursorResume::RotatedAt(0)
        } else if self.offset > active_length {
            CursorResume::TruncatedAt(0)
        } else {
            CursorResume::ContinueAt(self.offset)
        }
    }

    /// Registered source identifier.
    pub fn source_id(&self) -> Uuid {
        self.source_id
    }

    /// Stable non-path file identity.
    pub fn file_identity(&self) -> &str {
        &self.file_identity
    }

    /// Byte offset within the identified file.
    pub fn offset(&self) -> u64 {
        self.offset
    }
}

fn validate_identity(value: &str) -> Result<(), LogError> {
    if value.is_empty()
        || value.len() > 128
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.chars().any(char::is_control)
    {
        return Err(LogError::InvalidCursorIdentity);
    }
    Ok(())
}

/// Cursor recovery decision after inspecting the active registered source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorResume {
    /// Continue in the same file at this offset.
    ContinueAt(u64),
    /// The source rotated; begin the active file at this offset.
    RotatedAt(u64),
    /// The source was truncated; begin again at this offset.
    TruncatedAt(u64),
}

/// Remove secret-bearing request data and unsafe control characters.
pub fn redact_log_text(input: &str) -> String {
    input
        .lines()
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mask a remote address to its IPv4 /24 or IPv6 /64 network.
pub fn mask_remote_address(address: IpAddr) -> String {
    match address {
        IpAddr::V4(address) => {
            let [a, b, c, _] = address.octets();
            std::net::Ipv4Addr::new(a, b, c, 0).to_string()
        }
        IpAddr::V6(address) => {
            let [a, b, c, d, _, _, _, _] = address.segments();
            std::net::Ipv6Addr::new(a, b, c, d, 0, 0, 0, 0).to_string()
        }
    }
}

fn redact_line(line: &str) -> String {
    let cleaned: String = line
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\t')
        .collect();
    let lower = cleaned.trim_start().to_ascii_lowercase();
    if lower.starts_with("authorization:") || lower.starts_with("cookie:") {
        return cleaned.split_once(':').map_or_else(
            || "[REDACTED]".into(),
            |(name, _)| format!("{name}: [REDACTED]"),
        );
    }

    let without_query = match cleaned.find('?') {
        Some(start) => {
            let suffix = cleaned[start + 1..]
                .find(char::is_whitespace)
                .map(|relative| start + 1 + relative)
                .unwrap_or(cleaned.len());
            format!("{}?[REDACTED]{}", &cleaned[..start], &cleaned[suffix..])
        }
        None => cleaned,
    };
    redact_named_assignments(without_query)
}

fn redact_named_assignments(mut value: String) -> String {
    for key in ["password", "token", "secret", "api_key"] {
        let mut search_from = 0;
        loop {
            let lower = value.to_ascii_lowercase();
            let needle = format!("{key}=");
            let Some(relative_start) = lower[search_from..].find(&needle) else {
                break;
            };
            let start = search_from + relative_start;
            let value_start = start + needle.len();
            let value_end = value[value_start..]
                .find(|ch: char| ch.is_whitespace() || ch == '&' || ch == ';')
                .map(|relative| value_start + relative)
                .unwrap_or(value.len());
            value.replace_range(value_start..value_end, "[REDACTED]");
            search_from = value_start + "[REDACTED]".len();
        }
    }
    value
}

/// Parsed record emitted by OpenPanel's managed nginx access-log format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedAccessRecord {
    site_id: Uuid,
    timestamp: DateTime<Utc>,
    method: String,
    path: String,
    status: u16,
    response_bytes: u64,
    duration_ms: u64,
    remote_addr: IpAddr,
    user_agent: String,
}

impl ManagedAccessRecord {
    /// Parse one tab-delimited managed nginx access record.
    pub fn parse(line: &str) -> Result<Self, LogError> {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 9 {
            return Err(LogError::MalformedRecord);
        }
        let status = fields[4]
            .parse::<u16>()
            .map_err(|_| LogError::MalformedRecord)?;
        if !(100..=599).contains(&status) || fields[2].is_empty() || !fields[3].starts_with('/') {
            return Err(LogError::MalformedRecord);
        }
        Ok(Self {
            site_id: fields[0].parse().map_err(|_| LogError::MalformedRecord)?,
            timestamp: fields[1].parse().map_err(|_| LogError::MalformedRecord)?,
            method: fields[2].to_string(),
            path: fields[3].split('?').next().unwrap_or("/").to_string(),
            status,
            response_bytes: fields[5].parse().map_err(|_| LogError::MalformedRecord)?,
            duration_ms: parse_duration_ms(fields[6])?,
            remote_addr: fields[7].parse().map_err(|_| LogError::MalformedRecord)?,
            user_agent: redact_log_text(fields[8]),
        })
    }

    /// Site associated with this record.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Request timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// HTTP method.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Normalized path without a query string.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// HTTP response status.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Response size in bytes.
    pub fn response_bytes(&self) -> u64 {
        self.response_bytes
    }

    /// Request duration in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    /// Remote address.
    pub fn remote_addr(&self) -> IpAddr {
        self.remote_addr
    }

    /// Redacted user agent.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}

fn parse_duration_ms(value: &str) -> Result<u64, LogError> {
    if let Ok(milliseconds) = value.parse::<u64>() {
        return Ok(milliseconds);
    }
    let seconds = value
        .parse::<f64>()
        .map_err(|_| LogError::MalformedRecord)?;
    if !seconds.is_finite() || seconds.is_sign_negative() {
        return Err(LogError::MalformedRecord);
    }
    Ok((seconds * 1000.0).round().min(u64::MAX as f64) as u64)
}

/// Idempotency identity for one contiguous source byte range.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceBatch {
    source_id: Uuid,
    file_identity: String,
    start: u64,
    end: u64,
}

impl SourceBatch {
    /// Create a non-empty batch under a non-path file identity.
    pub fn new(
        source_id: Uuid,
        file_identity: impl Into<String>,
        start: u64,
        end: u64,
    ) -> Result<Self, LogError> {
        let file_identity = file_identity.into();
        validate_identity(&file_identity)?;
        if end <= start {
            return Err(LogError::InvalidSourceBatch);
        }
        Ok(Self {
            source_id,
            file_identity,
            start,
            end,
        })
    }
}

/// Metrics contributed by one managed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrafficSample {
    status: u16,
    response_bytes: u64,
    latency_ms: u64,
}

impl TrafficSample {
    /// Create a sample with a valid HTTP status.
    pub fn new(status: u16, response_bytes: u64, latency_ms: u64) -> Result<Self, LogError> {
        if !(100..=599).contains(&status) {
            return Err(LogError::InvalidTrafficSample);
        }
        Ok(Self {
            status,
            response_bytes,
            latency_ms,
        })
    }
}

/// One site's idempotent hourly traffic totals.
#[derive(Debug, Clone)]
pub struct TrafficAggregate {
    site_id: Uuid,
    hour: DateTime<Utc>,
    applied: HashSet<SourceBatch>,
    request_count: u64,
    response_bytes: u64,
    status_2xx: u64,
    status_3xx: u64,
    status_4xx: u64,
    status_5xx: u64,
    latency_ms_total: u64,
}

impl TrafficAggregate {
    /// Create an empty aggregate at an exact UTC hour boundary.
    pub fn new(site_id: Uuid, hour: DateTime<Utc>) -> Result<Self, LogError> {
        if hour.minute() != 0 || hour.second() != 0 || hour.nanosecond() != 0 {
            return Err(LogError::InvalidAggregateHour);
        }
        Ok(Self {
            site_id,
            hour,
            applied: HashSet::new(),
            request_count: 0,
            response_bytes: 0,
            status_2xx: 0,
            status_3xx: 0,
            status_4xx: 0,
            status_5xx: 0,
            latency_ms_total: 0,
        })
    }

    /// Apply a batch once, returning whether totals changed.
    pub fn apply(&mut self, batch: &SourceBatch, sample: &TrafficSample) -> bool {
        if !self.applied.insert(batch.clone()) {
            return false;
        }
        self.request_count = self.request_count.saturating_add(1);
        self.response_bytes = self.response_bytes.saturating_add(sample.response_bytes);
        self.latency_ms_total = self.latency_ms_total.saturating_add(sample.latency_ms);
        match sample.status / 100 {
            2 => self.status_2xx = self.status_2xx.saturating_add(1),
            3 => self.status_3xx = self.status_3xx.saturating_add(1),
            4 => self.status_4xx = self.status_4xx.saturating_add(1),
            5 => self.status_5xx = self.status_5xx.saturating_add(1),
            _ => {}
        }
        true
    }

    /// Request total.
    pub fn request_count(&self) -> u64 {
        self.request_count
    }

    /// Response byte total.
    pub fn response_bytes(&self) -> u64 {
        self.response_bytes
    }

    /// Successful response total.
    pub fn status_2xx(&self) -> u64 {
        self.status_2xx
    }

    /// Redirect response total.
    pub fn status_3xx(&self) -> u64 {
        self.status_3xx
    }

    /// Client-error response total.
    pub fn status_4xx(&self) -> u64 {
        self.status_4xx
    }

    /// Server-error response total.
    pub fn status_5xx(&self) -> u64 {
        self.status_5xx
    }

    /// Total latency in milliseconds.
    pub fn latency_ms_total(&self) -> u64 {
        self.latency_ms_total
    }

    /// Aggregate site.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Aggregate hour.
    pub fn hour(&self) -> DateTime<Utc> {
        self.hour
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn cursor_round_trip_is_opaque_and_cannot_encode_a_path() {
        let source_id = Uuid::new_v4();
        let cursor = LogCursor::new(source_id, "inode-42", 8192).unwrap();
        let encoded = cursor.encode().unwrap();

        assert!(!encoded.contains("inode-42"));
        assert_eq!(LogCursor::decode(&encoded).unwrap(), cursor);
        assert!(LogCursor::new(source_id, "../../etc/passwd", 0).is_err());
        assert!(LogCursor::new(source_id, "/var/log/nginx/access.log", 0).is_err());
    }

    #[test]
    fn cursor_detects_rotation_without_accepting_a_replacement_path() {
        let cursor = LogCursor::new(Uuid::new_v4(), "inode-42", 300).unwrap();
        assert_eq!(
            cursor.resume("inode-42", 400),
            CursorResume::ContinueAt(300)
        );
        assert_eq!(cursor.resume("inode-99", 20), CursorResume::RotatedAt(0));
        assert_eq!(cursor.resume("inode-42", 20), CursorResume::TruncatedAt(0));
    }

    #[test]
    fn redaction_removes_queries_credentials_headers_and_controls() {
        let input = "GET /login?token=secret&next=/ HTTP/1.1\nauthorization: Bearer abc\ncookie: sid=xyz\npassword=hunter2\u{0000}";
        let output = redact_log_text(input);

        assert!(!output.contains("secret"));
        assert!(!output.contains("abc"));
        assert!(!output.contains("xyz"));
        assert!(!output.contains("hunter2"));
        assert!(!output.contains('\u{0000}'));
        assert!(output.contains("/login?[REDACTED]"));
    }

    #[test]
    fn managed_access_parser_accepts_ipv4_ipv6_and_rejects_malformed_lines() {
        let site_id = Uuid::new_v4();
        let ipv4 = format!(
            "{site_id}\t2026-08-09T02:00:00Z\tGET\t/docs\t200\t512\t17\t192.0.2.1\tMozilla/5.0"
        );
        let record = ManagedAccessRecord::parse(&ipv4).unwrap();
        assert_eq!(record.site_id(), site_id);
        assert_eq!(
            record.remote_addr(),
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))
        );
        assert_eq!(record.path(), "/docs");

        let ipv6 =
            format!("{site_id}\t2026-08-09T02:00:00Z\tPOST\t/api\t503\t0\t31\t2001:db8::1\tagent");
        assert_eq!(
            ManagedAccessRecord::parse(&ipv6).unwrap().remote_addr(),
            IpAddr::V6("2001:db8::1".parse::<Ipv6Addr>().unwrap())
        );
        assert!(ManagedAccessRecord::parse("malformed").is_err());
        assert!(
            ManagedAccessRecord::parse(&format!(
                "{site_id}\tbad-time\tGET\t/\t200\t1\t1\t192.0.2.1\tagent"
            ))
            .is_err()
        );
    }

    #[test]
    fn remote_address_masking_preserves_network_only() {
        assert_eq!(
            mask_remote_address("192.0.2.129".parse().unwrap()),
            "192.0.2.0"
        );
        assert_eq!(
            mask_remote_address("2001:db8:abcd:12:3456:789a:bcde:f012".parse().unwrap()),
            "2001:db8:abcd:12::"
        );
    }

    #[test]
    fn traffic_aggregation_is_hourly_and_idempotent_per_source_batch() {
        let site_id = Uuid::new_v4();
        let hour = Utc.with_ymd_and_hms(2026, 8, 9, 2, 0, 0).unwrap();
        let mut aggregate = TrafficAggregate::new(site_id, hour).unwrap();
        let batch = SourceBatch::new(Uuid::new_v4(), "inode-42", 0, 100).unwrap();
        let sample = TrafficSample::new(200, 512, 17).unwrap();

        assert!(aggregate.apply(&batch, &sample));
        assert!(!aggregate.apply(&batch, &sample));
        assert_eq!(aggregate.request_count(), 1);
        assert_eq!(aggregate.response_bytes(), 512);
        assert_eq!(aggregate.status_2xx(), 1);
        assert_eq!(aggregate.latency_ms_total(), 17);
    }

    proptest! {
        #[test]
        fn prop_arbitrary_lines_never_panic(line in any::<String>()) {
            let _ = ManagedAccessRecord::parse(&line);
            let redacted = redact_log_text(&line);
            prop_assert!(!redacted.chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t'));
        }

        #[test]
        fn prop_cursor_file_identity_rejects_path_syntax(value in ".{0,4}[/\\\\].{0,40}") {
            prop_assert!(LogCursor::new(Uuid::new_v4(), value, 0).is_err());
        }

        #[test]
        fn prop_secret_query_value_is_removed(secret in "[A-Za-z0-9]{1,64}") {
            let redacted = redact_log_text(&format!("GET /?token={secret} HTTP/1.1"));
            prop_assert_eq!(redacted, "GET /?[REDACTED] HTTP/1.1");
        }
    }
}

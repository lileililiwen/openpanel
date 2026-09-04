//! Opaque pagination cursor for audit log queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Opaque, URL-safe pagination cursor encoding the last returned row's
/// `(ts, id)` so the next page continues deterministically, even when
/// many events share a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCursor {
    /// Timestamp of the boundary row.
    pub ts: DateTime<Utc>,
    /// Primary-key id of the boundary row.
    pub id: i64,
}

impl AuditCursor {
    /// Encode the cursor into a stable, non-sensitive string.
    pub fn encode(&self) -> String {
        encode_base64(format!("{}|{}", self.ts.to_rfc3339(), self.id).as_bytes())
    }

    /// Decode a cursor previously produced by [`AuditCursor::encode`].
    pub fn decode(s: &str) -> Option<Self> {
        let raw = decode_base64(s).ok()?;
        let text = String::from_utf8(raw).ok()?;
        let (ts_part, id_part) = text.split_once('|')?;
        let ts = DateTime::parse_from_rfc3339(ts_part)
            .ok()?
            .with_timezone(&Utc);
        let id = id_part.parse::<i64>().ok()?;
        Some(Self { ts, id })
    }
}

/// Filters applied to the audit log query.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Exact actor (user or service) identifier.
    pub actor: Option<String>,
    /// Exact action value (stored snake_case string, e.g. `site_created`).
    pub action: Option<String>,
    /// Substring match on the target the action was performed on.
    pub target: Option<String>,
    /// Exact outcome value (`success` / `failure` / `denied`).
    pub outcome: Option<String>,
    /// Inclusive lower bound on event timestamp.
    pub from: Option<DateTime<Utc>>,
    /// Inclusive upper bound on event timestamp.
    pub to: Option<DateTime<Utc>>,
    /// Cursor returned by a previous page; continues after it.
    pub cursor: Option<AuditCursor>,
    /// Maximum number of events to return (clamped to 1..=200).
    pub limit: usize,
}

impl AuditQuery {
    /// A fresh query returning the default page size.
    pub fn new() -> Self {
        Self {
            limit: 50,
            ..Default::default()
        }
    }

    /// Attach a decoded cursor, ignoring an unparseable value.
    pub fn with_cursor(mut self, cursor: Option<&str>) -> Self {
        self.cursor = cursor.and_then(AuditCursor::decode);
        self
    }

    /// Clamp `limit` into the supported `1..=200` range.
    pub fn effective_limit(&self) -> i64 {
        (self.limit.clamp(1, 200) as i64) + 1
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Minimal standard-base64 encoder (no external dependency).
fn encode_base64(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Minimal standard-base64 decoder (no external dependency).
fn decode_base64(input: &str) -> Result<Vec<u8>, ()> {
    let mut groups: Vec<u8> = Vec::new();
    for c in input.trim().chars() {
        if c == '=' {
            continue;
        }
        let v = B64.iter().position(|&x| x == c as u8).ok_or(())?;
        groups.push(v as u8);
    }
    let mut out = Vec::new();
    for chunk in groups.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let n = ((chunk[0] as u32) << 18)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 12)
            | ((chunk.get(2).copied().unwrap_or(0) as u32) << 6)
            | (chunk.get(3).copied().unwrap_or(0) as u32);
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

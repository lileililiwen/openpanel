//! Public status page read model + pure derivations.
//!
//! The status page is a slug-routed, unauthenticated view over the
//! existing `SyntheticCheck` / `CheckResult` data. The domain types
//! here describe the read model only — enabling, persisting, and
//! updating the page lives in the application layer.
//!
//! Two pure functions live here:
//!
//! - [`derive_incidents`] folds a sequence of `CheckResult`s into
//!   non-overlapping incidents per `check_id`.
//! - [`uptime_bars_90d`] buckets results into one bar per UTC day for
//!   the last 90 days (a day with no data renders as "no data",
//!   not 100%).

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CheckResult, CheckStatus, RepoError};

/// One entry on the status page: a published `SyntheticCheck` whose
/// `label` (chosen by the operator) is the only identifier shown to
/// anonymous visitors. The internal check id is preserved for the
/// projector but never rendered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEntry {
    /// Internal check id.
    pub check_id: Uuid,
    /// Operator-chosen label that replaces the target on the page.
    pub label: String,
}

/// Public status page aggregate. Slugged, opt-in, and identified by a
/// 128-bit random slug that the admin may rotate to invalidate the
/// previous URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusPage {
    /// Public URL slug.
    pub slug: Slug,
    /// Whether the page is currently published.
    pub enabled: bool,
    /// Entries (one per published check) in display order.
    pub entries: Vec<StatusEntry>,
}

impl StatusPage {
    /// Empty default (single empty entry list, slug unset).
    pub fn empty(slug: Slug) -> Self {
        Self {
            slug,
            enabled: false,
            entries: Vec::new(),
        }
    }
}

/// Enumeration-resistant random slug (128-bit base32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slug(String);

impl Slug {
    /// Wrap a candidate string as a slug after sanity-checking it.
    pub fn new(value: impl Into<String>) -> Result<Self, StatusPageError> {
        let value = value.into();
        if value.is_empty() || value.len() > 64 {
            return Err(StatusPageError::InvalidSlug);
        }
        if !value.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(StatusPageError::InvalidSlug);
        }
        Ok(Self(value))
    }

    /// As a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Errors raised by the status page bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatusPageError {
    /// The slug is malformed.
    #[error("invalid slug")]
    InvalidSlug,
    /// The check id has no corresponding `SyntheticCheck`.
    #[error("check not found: {0}")]
    CheckNotFound(Uuid),
    /// The status page is disabled.
    #[error("status page is disabled")]
    Disabled,
    /// Persistence failure.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<RepoError> for StatusPageError {
    fn from(error: RepoError) -> Self {
        StatusPageError::Persistence(error.0)
    }
}

/// Persistence port for the status page read model.
#[async_trait::async_trait]
pub trait StatusPageRepository: Send + Sync + 'static {
    /// Persist the page aggregate.
    async fn save(&self, page: &StatusPage) -> Result<(), StatusPageError>;
    /// Load the (single) page aggregate.
    async fn load(&self) -> Result<StatusPage, StatusPageError>;
}

/// A single incident derived from consecutive non-OK results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    /// Check the incident belongs to.
    pub check_id: Uuid,
    /// When the first non-OK result was observed.
    pub started_at: DateTime<Utc>,
    /// When the incident closed (first subsequent OK result).
    pub resolved_at: Option<DateTime<Utc>>,
    /// Outcome at close (or the latest observed status for open ones).
    pub outcome: CheckStatus,
}

/// One day's uptime bar: either a percentage (0.0–1.0) of OK results
/// in that UTC day, or `None` if there were no samples at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyBar {
    /// Day this bar covers.
    pub day: NaiveDate,
    /// Ratio of `Ok` results to total results in that day, or
    /// `None` when there were no results at all (renders as
    /// "no data" rather than 100%).
    pub uptime: Option<f32>,
}

/// Derive non-overlapping incidents from a sequence of `CheckResult`s
/// for a single check, ordered by `ran_at` ascending.
///
/// An incident opens on the first non-OK result and closes on the
/// first subsequent OK result. Consecutive non-OK results merge into a
/// single incident. An unresolved incident is returned with
/// `resolved_at = None`.
pub fn derive_incidents(results: &[CheckResult], check_id: Uuid) -> Vec<Incident> {
    let mut incidents = Vec::new();
    let mut current: Option<Incident> = None;
    for result in results.iter().filter(|r| r.check_id == check_id) {
        match result.status {
            CheckStatus::Ok => {
                if let Some(mut inc) = current.take() {
                    inc.resolved_at = Some(result.ran_at);
                    incidents.push(inc);
                }
            }
            CheckStatus::Warn | CheckStatus::Fail => {
                if current.is_none() {
                    current = Some(Incident {
                        check_id,
                        started_at: result.ran_at,
                        resolved_at: None,
                        outcome: result.status,
                    });
                } else if let Some(inc) = current.as_mut() {
                    // Keep the first start time and promote outcome on
                    // escalation (Warn -> Fail).
                    inc.outcome = promote(inc.outcome, result.status);
                }
            }
        }
    }
    if let Some(inc) = current {
        incidents.push(inc);
    }
    incidents
}

fn promote(prev: CheckStatus, next: CheckStatus) -> CheckStatus {
    let rank = |s: CheckStatus| match s {
        CheckStatus::Ok => 0,
        CheckStatus::Warn => 1,
        CheckStatus::Fail => 2,
    };
    if rank(next) > rank(prev) {
        next
    } else {
        prev
    }
}

/// Bucket a check's results into daily uptime bars for the last 90
/// UTC days (inclusive of `today`). Days with no samples yield
/// `uptime = None` so the renderer can distinguish "no data" from
/// 100%.
pub fn uptime_bars_90d(results: &[CheckResult], check_id: Uuid, today: NaiveDate) -> Vec<DailyBar> {
    let start = today - Duration::days(89);
    let mut buckets: Vec<(NaiveDate, u32, u32)> = (0..90)
        .map(|offset| {
            let day = start + Duration::days(offset);
            (day, 0, 0)
        })
        .collect();
    for result in results.iter().filter(|r| r.check_id == check_id) {
        let day = utc_date(result.ran_at);
        if day < start || day > today {
            continue;
        }
        let index = (day - start).num_days() as usize;
        if let Some(bucket) = buckets.get_mut(index) {
            bucket.1 += 1;
            if matches!(result.status, CheckStatus::Ok) {
                bucket.2 += 1;
            }
        }
    }
    buckets
        .into_iter()
        .map(|(day, total, ok)| {
            let uptime = if total == 0 {
                None
            } else {
                Some(ok as f32 / total as f32)
            };
            DailyBar { day, uptime }
        })
        .collect()
}

fn utc_date(ts: DateTime<Utc>) -> NaiveDate {
    Utc.timestamp_opt(ts.timestamp(), 0)
        .single()
        .map(|dt| dt.date_naive())
        .unwrap_or_else(|| {
            // Fall back to UTC components for out-of-range timestamps.
            let y = ts.year();
            let m = ts.month();
            let d = ts.day();
            NaiveDate::from_ymd_opt(y, m, d).unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(check_id: Uuid, status: CheckStatus, ran_at: DateTime<Utc>) -> CheckResult {
        CheckResult {
            id: Uuid::new_v4(),
            check_id,
            ran_at,
            latency_ms: 0,
            http_status: None,
            cert_days_remaining: None,
            status,
            message: String::new(),
        }
    }

    #[test]
    fn derive_incidents_empty_input_yields_none() {
        let id = Uuid::new_v4();
        assert!(derive_incidents(&[], id).is_empty());
    }

    #[test]
    fn derive_incidents_consecutive_failures_merge_into_one() {
        let id = Uuid::new_v4();
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let results = vec![
            result(id, CheckStatus::Ok, t0),
            result(id, CheckStatus::Warn, t0 + Duration::minutes(1)),
            result(id, CheckStatus::Fail, t0 + Duration::minutes(2)),
            result(id, CheckStatus::Fail, t0 + Duration::minutes(3)),
            result(id, CheckStatus::Ok, t0 + Duration::minutes(4)),
        ];
        let inc = derive_incidents(&results, id);
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].started_at, t0 + Duration::minutes(1));
        assert_eq!(inc[0].resolved_at, Some(t0 + Duration::minutes(4)));
        assert_eq!(inc[0].outcome, CheckStatus::Fail);
    }

    #[test]
    fn derive_incidents_unresolved_has_no_resolved_at() {
        let id = Uuid::new_v4();
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let results = vec![
            result(id, CheckStatus::Warn, t0),
            result(id, CheckStatus::Fail, t0 + Duration::minutes(1)),
        ];
        let inc = derive_incidents(&results, id);
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].resolved_at, None);
    }

    #[test]
    fn uptime_bars_90d_no_data_renders_as_none() {
        let id = Uuid::new_v4();
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let bars = uptime_bars_90d(&[], id, today);
        assert_eq!(bars.len(), 90);
        assert!(bars.iter().all(|b| b.uptime.is_none()));
    }

    #[test]
    fn uptime_bars_90d_buckets_by_utc_day() {
        let id = Uuid::new_v4();
        let today = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap();
        let results = vec![
            result(
                id,
                CheckStatus::Ok,
                Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
            ),
            result(
                id,
                CheckStatus::Fail,
                Utc.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).unwrap(),
            ),
            result(
                id,
                CheckStatus::Ok,
                Utc.with_ymd_and_hms(2026, 1, 3, 12, 0, 0).unwrap(),
            ),
        ];
        let bars = uptime_bars_90d(&results, id, today);
        // 90 bars total; days 1, 2, 3 have data, the rest are None.
        assert_eq!(bars.len(), 90);
        assert!(bars.iter().any(|b| b.day == today && b.uptime == Some(1.0)));
        assert!(
            bars.iter()
                .any(|b| b.day == NaiveDate::from_ymd_opt(2026, 1, 2).unwrap() && b.uptime == Some(0.0))
        );
        assert!(
            bars.iter()
                .any(|b| b.day == NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() && b.uptime == Some(1.0))
        );
        assert!(
            bars.iter()
                .any(|b| b.day == NaiveDate::from_ymd_opt(2025, 12, 30).unwrap() && b.uptime.is_none())
        );
    }

    #[test]
    fn slug_validates_alphanumeric_only() {
        assert!(Slug::new("abc123XYZ").is_ok());
        assert!(Slug::new("").is_err());
        assert!(Slug::new("with-dash").is_err());
        assert!(Slug::new("with space").is_err());
        assert!(Slug::new(&"x".repeat(65)).is_err());
    }
}

#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn slug_round_trips_through_random_entropy(
            bytes in proptest::collection::vec(any::<u8>(), 16..=16)
        ) {
            // Map 16 bytes -> 25 base32 chars (no padding) — matches
            // RFC 4648 base32-nopadding exactly.
            const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
            let mut out = String::with_capacity(26);
            let mut buffer: u64 = 0;
            let mut bits: u32 = 0;
            for byte in bytes.iter() {
                buffer = (buffer << 8) | u64::from(*byte);
                bits += 8;
                while bits >= 5 {
                    bits -= 5;
                    let idx = ((buffer >> bits) & 0x1f) as usize;
                    out.push(ALPHABET[idx] as char);
                }
            }
            if bits > 0 {
                let idx = ((buffer <<(5 - bits)) & 0x1f) as usize;
                out.push(ALPHABET[idx] as char);
            }
            prop_assert!(Slug::new(&out).is_ok());
        }

        #[test]
        fn derive_incidents_never_overlap_or_invert(
            count in 0usize..64
        ) {
            use proptest::strategy::ValueTree;
            let check_id = Uuid::new_v4();
            let strat = proptest::collection::vec(
                prop::sample::select(&[CheckStatus::Ok, CheckStatus::Warn, CheckStatus::Fail]),
                count,
            );
            let statuses: Vec<CheckStatus> = strat
                .new_tree(&mut proptest::test_runner::TestRunner::default())
                .expect("tree")
                .current();
            let base = Utc::now();
            let results: Vec<CheckResult> = statuses
                .iter()
                .enumerate()
                .map(|(i, s)| CheckResult {
                    id: Uuid::new_v4(),
                    check_id,
                    ran_at: base + Duration::seconds(i as i64),
                    latency_ms: 0,
                    http_status: None,
                    cert_days_remaining: None,
                    status: *s,
                    message: String::new(),
                })
                .collect();
            let inc = derive_incidents(&results, check_id);
            for window in inc.windows(2) {
                prop_assert!(window[0].started_at <= window[1].started_at);
                let first_close = window[0].resolved_at.unwrap_or(window[0].started_at);
                prop_assert!(first_close <= window[1].started_at);
            }
            for i in &inc {
                if let Some(end) = i.resolved_at {
                    prop_assert!(i.started_at < end);
                }
            }
        }
    }
}
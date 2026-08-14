//! `LogSeq`, `RestoreTimestamp`, and `BinlogRange` value objects.
//!
//! A `LogSeq` is the monotonically increasing identifier a database
//! engine uses to address a position in its transaction log
//! (MySQL/MariaDB binlog offset, PostgreSQL WAL LSN, etc.). The
//! number is opaque to the domain — it is preserved as bytes so the
//! engine can compare two positions for "is A at or before B?" without
//! the domain having to know the layout.
//!
//! `RestoreTimestamp` is the wall-clock instant a caller wants a
//! database restored to. `BinlogRange` is the closed interval of
//! `LogSeq` values plus the wall-clock window they cover, exposed by
//! the inspection endpoint so the caller can pick a valid timestamp.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db_pitr::error::PitrError;

/// Engine-side transaction-log position.
///
/// Internally the value is a `Vec<u8>` so the domain does not have to
/// know the layout of MySQL GTIDs, MariaDB binlog offsets, or
/// PostgreSQL WAL LSNs. The ordering rule "is `a` at or before `b`?"
/// is delegated to the engine adapter; in the domain, two `LogSeq`
/// values are equal iff their bytes are equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogSeq(Vec<u8>);

impl LogSeq {
    /// Engine-initial position (e.g. "the start of the log").
    pub fn initial() -> Self {
        Self(Vec::new())
    }

    /// Build a `LogSeq` from raw engine bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, PitrError> {
        if bytes.len() > 64 {
            return Err(PitrError::Invalid(format!(
                "log sequence number is {} bytes; max 64",
                bytes.len()
            )));
        }
        Ok(Self(bytes))
    }

    /// Borrow the raw engine bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Hex representation suitable for logs and storage.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    /// Parse a hex string into a `LogSeq`.
    pub fn from_hex(s: &str) -> Result<Self, PitrError> {
        let trimmed = s.trim_start_matches("0x");
        let bytes = hex::decode(trimmed)
            .map_err(|e| PitrError::Invalid(format!("invalid log sequence hex: {e}")))?;
        Self::from_bytes(bytes)
    }
}

impl fmt::Display for LogSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Wall-clock instant a caller wants the database restored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RestoreTimestamp(DateTime<Utc>);

impl RestoreTimestamp {
    /// Build a restore timestamp. The instant MUST be in the past;
    /// future timestamps are rejected because the engine cannot
    /// restore state at a moment that has not yet happened.
    pub fn new(ts: DateTime<Utc>) -> Result<Self, PitrError> {
        if ts > Utc::now() {
            return Err(PitrError::Invalid(format!(
                "restore timestamp {} is in the future",
                ts.to_rfc3339()
            )));
        }
        Ok(Self(ts))
    }

    /// Build from an existing stored value (e.g. on restore), without
    /// enforcing the future-timestamp rule — the row was already
    /// accepted at creation time.
    pub fn from_stored(ts: DateTime<Utc>) -> Self {
        Self(ts)
    }

    /// The underlying instant.
    pub fn as_datetime(&self) -> DateTime<Utc> {
        self.0
    }
}

impl fmt::Display for RestoreTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.to_rfc3339().fmt(f)
    }
}

/// Closed interval of available transaction-log positions plus the
/// wall-clock window they cover. Returned by the inspection endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinlogRange {
    /// Earliest available log sequence.
    pub earliest: LogSeq,
    /// Latest available log sequence.
    pub latest: LogSeq,
    /// Wall-clock instant the earliest segment was generated at.
    pub start: DateTime<Utc>,
    /// Wall-clock instant the latest segment was generated at.
    pub end: DateTime<Utc>,
    /// True iff no log segments are available for this database yet.
    pub empty: bool,
}

impl BinlogRange {
    /// Build a range. Requires `end >= start` and the `LogSeq`s to be
    /// well-formed (which `LogSeq::from_bytes` already guarantees).
    pub fn new(
        earliest: LogSeq,
        latest: LogSeq,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Self, PitrError> {
        if end < start {
            return Err(PitrError::Invalid(
                "binlog range end is before start".into(),
            ));
        }
        let empty = earliest == latest && earliest == LogSeq::initial();
        Ok(Self {
            earliest,
            latest,
            start,
            end,
            empty,
        })
    }

    /// True iff `ts` falls within the wall-clock window. The caller
    /// is responsible for picking a timestamp in the past; the
    /// endpoint uses this helper to flag "unrestorable" timestamps.
    pub fn covers(&self, ts: DateTime<Utc>) -> bool {
        !self.empty && ts >= self.start && ts <= self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_seq_hex_round_trip() {
        let seq = LogSeq::from_bytes(vec![0x01, 0x02, 0x0f]).unwrap();
        assert_eq!(seq.to_hex(), "01020f");
        let parsed = LogSeq::from_hex("0x01020f").unwrap();
        assert_eq!(parsed, seq);
    }

    #[test]
    fn log_seq_rejects_oversize() {
        let big = vec![0u8; 65];
        assert!(matches!(
            LogSeq::from_bytes(big),
            Err(PitrError::Invalid(_))
        ));
    }

    #[test]
    fn restore_timestamp_rejects_future() {
        let future = Utc::now() + chrono::Duration::seconds(60);
        assert!(matches!(
            RestoreTimestamp::new(future),
            Err(PitrError::Invalid(_))
        ));
    }

    #[test]
    fn binlog_range_constructed_empty() {
        let now = Utc::now();
        let range = BinlogRange::new(LogSeq::initial(), LogSeq::initial(), now, now).unwrap();
        assert!(range.empty);
        assert!(!range.covers(now + chrono::Duration::seconds(1)));
    }

    #[test]
    fn binlog_range_covers_inclusive() {
        let start = Utc::now() - chrono::Duration::seconds(60);
        let end = Utc::now();
        let earliest = LogSeq::from_bytes(vec![0x01]).unwrap();
        let latest = LogSeq::from_bytes(vec![0x09]).unwrap();
        let range = BinlogRange::new(earliest, latest, start, end).unwrap();
        assert!(!range.empty);
        assert!(range.covers(start));
        assert!(range.covers(end));
        let before = start - chrono::Duration::seconds(1);
        assert!(!range.covers(before));
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn prop_log_seq_hex_round_trip(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            let seq = LogSeq::from_bytes(bytes.clone()).unwrap();
            let hex = seq.to_hex();
            let parsed = LogSeq::from_hex(&hex).unwrap();
            prop_assert_eq!(parsed, seq);
        }

        #[test]
        fn prop_log_seq_oversize_rejected(
            bytes in proptest::collection::vec(any::<u8>(), 65..96)
        ) {
            prop_assert!(matches!(
                LogSeq::from_bytes(bytes),
                Err(PitrError::Invalid(_))
            ));
        }

        #[test]
        fn prop_binlog_range_end_before_start_rejected(
            base_secs in 1_000_000_000i64..2_000_000_000,
            delta in 1i64..1_000
        ) {
            // end is *before* start, by `delta` seconds
            let start_ts = chrono::DateTime::<Utc>::from_timestamp(base_secs, 0).unwrap();
            let end_ts = chrono::DateTime::<Utc>::from_timestamp(base_secs - delta, 0).unwrap();
            let r = BinlogRange::new(
                LogSeq::initial(),
                LogSeq::initial(),
                start_ts,
                end_ts,
            );
            prop_assert!(matches!(r, Err(PitrError::Invalid(_))));
        }
    }
}

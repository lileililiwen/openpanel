//! `SnapshotRepository` — the port through which the application
//! service persists and retrieves metric samples.
//!
//! Implementation lives in `openpanel-app/src/monitoring/repo.rs`
//! (`SqliteSnapshotRepository`). Tests use the in-memory fake from
//! `openpanel-test-support` or hand-rolled `Arc<Mutex<HashMap>>`
//! fakes.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{RepoError, monitoring::metric::MetricSample};

/// Persistence port for metric samples (a single row per
/// `(timestamp, kind)`).
///
/// The time series is append-only: inserting a `(ts, kind)` pair that
/// already exists returns a `RepoError` whose message contains
/// `"unique violation"`.
#[async_trait]
pub trait SnapshotRepository: Send + Sync + 'static {
    /// Insert one sample. Returns a `RepoError` with a
    /// `"unique violation"` message if the same `(timestamp, kind)`
    /// already exists.
    async fn insert(&self, sample: &MetricSample) -> Result<(), RepoError>;

    /// Most recent sample of every kind (one row per kind, the row
    /// with the max timestamp for that kind).
    async fn latest(&self) -> Result<Vec<MetricSample>, RepoError>;

    /// All samples of one kind newer than `since`, ascending by
    /// timestamp.
    async fn history(
        &self,
        kind: crate::monitoring::metric::MetricKind,
        since: DateTime<Utc>,
    ) -> Result<Vec<MetricSample>, RepoError>;

    /// Delete all samples with `timestamp < before`. Returns the
    /// number of rows deleted.
    async fn prune(&self, before: DateTime<Utc>) -> Result<u64, RepoError>;
}

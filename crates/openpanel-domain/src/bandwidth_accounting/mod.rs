//! Bandwidth accounting bounded context: typed bridge between the
//! `BandwidthObserver` collector (defined in `monitoring`) and
//! the SQLite-backed rolling-window store.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    RepoError,
    monitoring::{BandwidthObserver, BandwidthPeriod, BandwidthWindow},
};

/// Read-side query for rolling bandwidth windows.
#[async_trait]
pub trait BandwidthReader: Send + Sync + 'static {
    /// Return the rolling windows for `owner_id` in `period`.
    async fn rolling(
        &self,
        owner_id: Uuid,
        period: BandwidthPeriod,
    ) -> Vec<BandwidthWindow>;
}

/// A persisted counter row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandwidthCounterRow {
    pub period_kind: String,
    pub owner_id: Uuid,
    pub site_id: Option<Uuid>,
    pub period_starts_at: DateTime<Utc>,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub updated_at: DateTime<Utc>,
}

/// Persistence port for the bandwidth counters.
#[async_trait]
pub trait BandwidthRepository: Send + Sync + 'static {
    /// Increment or insert the counter for `(period, owner,
    /// site, period_starts_at)`.
    async fn increment(
        &self,
        period: BandwidthPeriod,
        owner_id: Uuid,
        site_id: Option<Uuid>,
        period_starts_at: DateTime<Utc>,
        bytes_in: u64,
        bytes_out: u64,
    ) -> Result<(), RepoError>;
    /// Read rolling windows for `owner_id`.
    async fn windows_for_owner(
        &self,
        owner_id: Uuid,
        period: BandwidthPeriod,
    ) -> Vec<BandwidthWindow>;
    /// Read rolling windows for `site_id`.
    async fn windows_for_site(
        &self,
        site_id: Uuid,
        period: BandwidthPeriod,
    ) -> Vec<BandwidthWindow>;
}

/// Storage observer that translates `BandwidthObserver` callbacks
/// into repository updates. The default impl is a no-op; the
/// follow-on storage layer wires the SQLite-backed repository.
pub struct BandwidthStorageObserver<R: BandwidthRepository> {
    repo: R,
}

impl<R: BandwidthRepository> BandwidthStorageObserver<R> {
    /// Build a new observer.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
    /// Compute the period start for `now` and `period`.
    pub fn period_start_for(
        period: BandwidthPeriod,
        now: DateTime<Utc>,
    ) -> DateTime<Utc> {
        use chrono::Datelike;
        use chrono::Timelike;
        match period {
            BandwidthPeriod::Hourly => {
                now.with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap()
            }
            BandwidthPeriod::Daily => {
                now.with_hour(0).unwrap().with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap()
            }
            BandwidthPeriod::Monthly => {
                now.with_day(1).unwrap().with_hour(0).unwrap().with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap()
            }
        }
    }
}

impl<R: BandwidthRepository> BandwidthObserver for BandwidthStorageObserver<R> {
    fn on_byte(&self, owner: Uuid, site: Option<Uuid>, in_: u64, out: u64) {
        // The synchronous observer hook cannot .await; the
        // follow-on stores the increment through a runtime that
        // owns the pool. The default impl is a no-op to keep
        // tests synchronous.
        let _ = (owner, site, in_, out);
    }
    fn on_window_close(&self, _window: &BandwidthWindow) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn hourly_period_start_truncates_to_hour() {
        let now = Utc.with_ymd_and_hms(2026, 8, 13, 10, 45, 30).unwrap();
        let start = BandwidthStorageObserver::<NoopRepo>::period_start_for(
            BandwidthPeriod::Hourly,
            now,
        );
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 13, 10, 0, 0).unwrap());
    }

    #[test]
    fn daily_period_start_truncates_to_day() {
        let now = Utc.with_ymd_and_hms(2026, 8, 13, 10, 45, 30).unwrap();
        let start = BandwidthStorageObserver::<NoopRepo>::period_start_for(
            BandwidthPeriod::Daily,
            now,
        );
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 13, 0, 0, 0).unwrap());
    }

    #[test]
    fn monthly_period_start_truncates_to_first() {
        let now = Utc.with_ymd_and_hms(2026, 8, 13, 10, 45, 30).unwrap();
        let start = BandwidthStorageObserver::<NoopRepo>::period_start_for(
            BandwidthPeriod::Monthly,
            now,
        );
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap());
    }

    struct NoopRepo;
    #[async_trait]
    impl BandwidthRepository for NoopRepo {
        async fn increment(
            &self,
            _period: BandwidthPeriod,
            _owner_id: Uuid,
            _site_id: Option<Uuid>,
            _period_starts_at: DateTime<Utc>,
            _bytes_in: u64,
            _bytes_out: u64,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn windows_for_owner(
            &self,
            _owner_id: Uuid,
            _period: BandwidthPeriod,
        ) -> Vec<BandwidthWindow> {
            Vec::new()
        }
        async fn windows_for_site(
            &self,
            _site_id: Uuid,
            _period: BandwidthPeriod,
        ) -> Vec<BandwidthWindow> {
            Vec::new()
        }
    }
}

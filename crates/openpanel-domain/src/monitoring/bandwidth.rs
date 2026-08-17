//! Bandwidth accounting value objects and the `BandwidthObserver`
//! collector plugin contract.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Aggregation period for a bandwidth window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandwidthPeriod {
    /// Hourly aggregation.
    Hourly,
    /// Daily aggregation.
    Daily,
    /// Monthly aggregation.
    Monthly,
}

impl BandwidthPeriod {
    /// Compute the next window start from `current`.
    pub fn next_start(&self, current: DateTime<Utc>) -> DateTime<Utc> {
        use chrono::Duration;
        match self {
            BandwidthPeriod::Hourly => current + Duration::hours(1),
            BandwidthPeriod::Daily => current + Duration::days(1),
            BandwidthPeriod::Monthly => {
                // Move to the first of the next month, UTC.
                if current.month() == 12 {
                    current
                        .with_year(current.year() + 1)
                        .unwrap()
                        .with_month(1)
                        .unwrap()
                } else {
                    current.with_month(current.month() + 1).unwrap()
                }
            }
        }
    }
}

/// A bandwidth window over a fixed period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandwidthWindow {
    pub owner_id: Uuid,
    pub site_id: Option<Uuid>,
    pub period: BandwidthPeriod,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

impl BandwidthWindow {
    /// Build a new window.
    pub fn new(
        owner_id: Uuid,
        site_id: Option<Uuid>,
        period: BandwidthPeriod,
        starts_at: DateTime<Utc>,
    ) -> Self {
        let ends_at = period.next_start(starts_at);
        Self {
            owner_id,
            site_id,
            period,
            starts_at,
            ends_at,
            bytes_in: 0,
            bytes_out: 0,
        }
    }

    /// Whether `now` is past the window's end.
    pub fn is_closed_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.ends_at
    }

    /// Total bytes (in + out).
    pub fn total_bytes(&self) -> u64 {
        self.bytes_in.saturating_add(self.bytes_out)
    }

    /// Compute the percentage of `limit` consumed by `self`.
    /// Returns 0 when `limit` is zero (avoids divide-by-zero).
    pub fn pct_used(&self, limit: u64) -> f32 {
        if limit == 0 {
            0.0
        } else {
            (self.total_bytes() as f64 / limit as f64 * 100.0) as f32
        }
    }

    /// Whether a 80% threshold has been crossed.
    pub fn is_over_80(&self, limit: u64) -> bool {
        self.pct_used(limit) >= 80.0
    }

    /// Whether a 100% threshold has been crossed.
    pub fn is_over_100(&self, limit: u64) -> bool {
        self.pct_used(limit) >= 100.0
    }
}

/// Per-account counter used to attribute traffic to a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandwidthCounter {
    pub owner_id: Uuid,
    pub site_id: Option<Uuid>,
    pub period: BandwidthPeriod,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

impl BandwidthCounter {
    /// Build a new counter.
    pub fn new(owner_id: Uuid, site_id: Option<Uuid>, period: BandwidthPeriod) -> Self {
        Self {
            owner_id,
            site_id,
            period,
            bytes_in: 0,
            bytes_out: 0,
        }
    }
    /// Add bytes to the counter.
    pub fn add(&mut self, in_: u64, out: u64) {
        self.bytes_in = self.bytes_in.saturating_add(in_);
        self.bytes_out = self.bytes_out.saturating_add(out);
    }
}

/// Collector plugin contract. The monitoring runtime dispatches
/// every byte transition to the registered observers; the
/// `add-bandwidth-accounting` follow-on change adds a
/// SQLite-backed implementation.
pub trait BandwidthObserver: Send + Sync {
    /// Called on every byte transition.
    fn on_byte(&self, owner: Uuid, site: Option<Uuid>, in_: u64, out: u64);
    /// Called when a window closes.
    fn on_window_close(&self, window: &BandwidthWindow);
}

/// A no-op observer used when no collector is registered.
pub struct NoopBandwidthObserver;

impl BandwidthObserver for NoopBandwidthObserver {
    fn on_byte(&self, _owner: Uuid, _site: Option<Uuid>, _in_: u64, _out: u64) {}
    fn on_window_close(&self, _window: &BandwidthWindow) {}
}

/// A small fan-out that drives every registered observer.
#[derive(Default)]
pub struct BandwidthObserverFanout {
    observers: Vec<Box<dyn BandwidthObserver>>,
}

impl BandwidthObserverFanout {
    /// Build an empty fanout.
    pub fn new() -> Self {
        Self::default()
    }
    /// Register an observer.
    pub fn register(&mut self, observer: Box<dyn BandwidthObserver>) {
        self.observers.push(observer);
    }
    /// Registered observers.
    pub fn observers(&self) -> &[Box<dyn BandwidthObserver>] {
        &self.observers
    }
}

impl BandwidthObserver for BandwidthObserverFanout {
    fn on_byte(&self, owner: Uuid, site: Option<Uuid>, in_: u64, out: u64) {
        for observer in &self.observers {
            observer.on_byte(owner, site, in_, out);
        }
    }
    fn on_window_close(&self, window: &BandwidthWindow) {
        for observer in &self.observers {
            observer.on_window_close(window);
        }
    }
}

/// A typed event: a threshold was crossed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandwidthThresholdCrossed {
    pub owner_id: Uuid,
    pub site_id: Option<Uuid>,
    pub period: BandwidthPeriod,
    pub pct_used: f32,
    pub limit_bytes: u64,
}

/// A typed event: a window closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandwidthWindowClosed {
    pub window: BandwidthWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_next_start_advances_correctly() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-13T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            BandwidthPeriod::Hourly.next_start(now),
            chrono::DateTime::parse_from_rfc3339("2026-08-13T11:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            BandwidthPeriod::Daily.next_start(now),
            chrono::DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        let dec = chrono::DateTime::parse_from_rfc3339("2026-12-15T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            BandwidthPeriod::Monthly.next_start(dec),
            chrono::DateTime::parse_from_rfc3339("2027-01-15T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn window_pct_used_is_monotonic() {
        let now = Utc::now();
        let mut w = BandwidthWindow::new(Uuid::new_v4(), None, BandwidthPeriod::Hourly, now);
        w.bytes_in = 50;
        w.bytes_out = 30;
        let mut previous = 0.0_f32;
        for _ in 0..10 {
            w.bytes_in = w.bytes_in.saturating_add(10);
            let pct = w.pct_used(1000);
            assert!(pct >= previous, "pct_used must be monotonic");
            previous = pct;
        }
    }

    #[test]
    fn window_threshold_flags() {
        let now = Utc::now();
        let mut w = BandwidthWindow::new(Uuid::new_v4(), None, BandwidthPeriod::Hourly, now);
        w.bytes_in = 800;
        w.bytes_out = 0;
        assert!(w.is_over_80(1000));
        assert!(!w.is_over_100(1000));
        w.bytes_in = 1000;
        assert!(w.is_over_100(1000));
    }

    #[test]
    fn window_pct_used_handles_zero_limit() {
        let now = Utc::now();
        let w = BandwidthWindow::new(Uuid::new_v4(), None, BandwidthPeriod::Hourly, now);
        assert_eq!(w.pct_used(0), 0.0);
    }

    #[test]
    fn counter_saturates_instead_of_overflowing() {
        let mut counter = BandwidthCounter::new(Uuid::new_v4(), None, BandwidthPeriod::Hourly);
        counter.add(u64::MAX, u64::MAX);
        counter.add(1, 1);
        assert_eq!(counter.bytes_in, u64::MAX);
        assert_eq!(counter.bytes_out, u64::MAX);
    }

    #[test]
    fn noop_observer_does_not_panic() {
        struct Owner;
        let _ = Owner;
        let observer = NoopBandwidthObserver;
        observer.on_byte(Uuid::new_v4(), None, 1, 2);
        let window = BandwidthWindow::new(Uuid::new_v4(), None, BandwidthPeriod::Hourly, Utc::now());
        observer.on_window_close(&window);
    }

    #[test]
    fn fanout_drives_all_observers() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        struct Counting(Arc<AtomicU64>);
        impl BandwidthObserver for Counting {
            fn on_byte(&self, _owner: Uuid, _site: Option<Uuid>, in_: u64, _out: u64) {
                self.0.fetch_add(in_, Ordering::SeqCst);
            }
            fn on_window_close(&self, _window: &BandwidthWindow) {}
        }
        let shared = Arc::new(AtomicU64::new(0));
        let mut fanout = BandwidthObserverFanout::new();
        fanout.register(Box::new(Counting(shared.clone())));
        fanout.register(Box::new(Counting(shared.clone())));
        fanout.on_byte(Uuid::new_v4(), None, 100, 0);
        assert_eq!(shared.load(Ordering::SeqCst), 200);
    }
}

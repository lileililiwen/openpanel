# Refine monitoring with bandwidth accounting and quota thresholds — Design

## Types

```rust
pub struct BandwidthWindow {
    pub owner_id: UserId,
    pub site_id: Option<SiteId>,
    pub period: BandwidthPeriod,            // Hourly | Daily | Monthly
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

pub enum BandwidthPeriod { Hourly, Daily, Monthly }

pub trait BandwidthObserver: Send + Sync {
    fn on_byte(&self, owner: UserId, site: Option<SiteId>, in_: u64, out: u64);
    fn on_window_close(&self, w: BandwidthWindow);
}
```

A factory wires the monitoring runtime to one or more observers
(no-op in this change). The follow-on `add-bandwidth-accounting`
change adds a SQLite-backed observer and a notification-channel
consumer.

## Events

```rust
domain_event!(BandwidthThresholdCrossed {
    owner_id: UserId,
    site_id: Option<SiteId>,
    period: BandwidthPeriod,
    pct_used: f32,        // 80, 100 typically
    limit_bytes: u64
});
domain_event!(BandwidthWindowClosed {
    window: BandwidthWindow
});
```

`pct_used` only carries the numeric threshold so a tamper-resistant
audit can match an external threshold policy without exposing the
configuration.

## Tests

```
1.1  Unit: BandwidthWindow math; period rotation; observer hook
      is callable.
1.2  Property: pct_used = bytes_total / limit * 100 is monotonic
      in bytes; period never overlaps itself.
1.3  Service tests with mock observer: every byte transition is
      recorded; window close is invoked once per period.
```

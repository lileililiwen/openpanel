# Add bandwidth accounting — Design

## Storage

```sql
CREATE TABLE bandwidth_counters (
    period_kind     TEXT NOT NULL CHECK (period_kind IN ('hourly','daily','monthly')),
    owner_id        TEXT NOT NULL,
    site_id         TEXT,
    period_starts   INTEGER NOT NULL,
    bytes_in        INTEGER NOT NULL,
    bytes_out       INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (period_kind, owner_id, site_id, period_starts)
);
CREATE INDEX idx_bandwidth_owners ON bandwidth_counters(period_kind, owner_id, period_starts);
CREATE INDEX idx_bandwidth_sites  ON bandwidth_counters(period_kind, site_id,  period_starts);
```

## Observer behaviour

```rust
impl BandwidthObserver for BandwidthStorageObserver {
    fn on_byte(&self, owner: UserId, site: Option<SiteId>, in_: u64, out: u64) {
        for period in [Hourly, Daily, Monthly] {
            increment_or_insert_period(period, owner, site, in_, out);
        }
    }
}
```

A nightly rollover routine closes prior period rows and emits
`BandwidthWindowClosed` events.

## Reader

```rust
trait BandwidthReader {
    async fn rolling(
        &self,
        principal: Principal,
        target: BandwidthTarget,        // Owner(UserId) | Site(SiteId)
        period: BandwidthPeriod,
        from: DateTime<Utc>, to: DateTime<Utc>,
    ) -> Vec<BandwidthWindow>;
}
```

Access control: only an Owner / Admin may read another
owner's totals; a User may read their own.

## ThresholdEvaluator

```
threshold_policy = QuotaPolicy::load(owner) -> Vec<AxisThreshold>
  AxisThreshold { axis: BandwidthBytesPerMonth, pct: 80, … }
```

The consumer subscribes to `BandwidthThresholdCrossed` events
emitted by the monitor. For each event it computes the
notification recipient (the owner or their delegate) and the
notification channel list as configured by
`notification-channels`, then emits a delivery event.

## Endpoints

```
GET  /api/v1/bandwidth/owners/{id}?period=daily&from=…&to=…
GET  /api/v1/bandwidth/sites/{id}?period=daily&from=…&to=…
```

## CLI

```
openpanel bandwidth show owner <id> --period daily
openpanel bandwidth show site  <id> --period monthly --from <ts>
```

## Tests

```
1.1  Unit: increment_or_insert merges into existing row;
      rollover emits BandwidthWindowClosed.
1.2  Property: total over multiple periods = sum of component
      rows; idempotent on_byte never inflates counters twice.
1.3  Service tests with mock monitor and observer: counter
      row exists after a byte transition; threshold evaluator
      dispatches at 80%.
1.4  Integration: live traffic through nginx + bytes recorded;
      reader returns the rolling totals.
1.5  CLI E2E: show / rolling window.
1.6  Web: bandwidth page with stacked chart and per-period
      table.
```

# Add bandwidth accounting

## Why

`refine-monitoring-with-bandwidth-accounting` introduced
bandwidth counter shapes and the observer hook but did not
persist counters or compute quota thresholds. The proposed
`add-resource-quotas` and `add-notification-channels` changes
need a real `BandwidthReader` that returns the rolling-window
bytes for a `(owner, period)`. cPanel's bandwidth meter and
Baota's per-site bandwidth view both compute and surface this.
This change implements the SQLite-backed observer and the
threshold-crossing consumer.

## What Changes

- New bounded context `bandwidth-accounting` carrying the
  `BandwidthStorageObserver` adapter.
- Storage: SQLite table `bandwidth_counters(period_kind,
  owner_id, site_id, period_starts_at, bytes_in, bytes_out)`.
- New `BandwidthReader` API serving rolling windows
  (Hourly/Daily/Monthly) for an owner or site.
- New `BandwidthThresholdEvaluator` consuming
  `BandwidthThresholdCrossed` events and dispatching to
  notification-channels.
- New endpoints: `GET /bandwidth/owners/{id}` and
  `GET /bandwidth/sites/{id}` returning rolling windows.

## Capabilities

### New Capabilities

- `bandwidth-accounting`: rolling-window storage and threshold
  evaluation for owner / site bandwidth.

## Impact

- Domain: `BandwidthReader`, `BandwidthThresholdEvaluator`,
  `BandwidthWindowKey`.
- App: `BandwidthStorageObserver`, `BandwidthReaderService`,
  `BandwidthThresholdConsumer`.
- API/CLI/web: `/bandwidth/{owners,sites}/*`; CLI
  `openpanel bandwidth show`; web `/bandwidth` page.
- Coupling: depends on `refine-monitoring-with-bandwidth-accounting`
  for the observer contract and on `add-resource-quotas` (and
  the proposed notification-channels) for threshold policy.

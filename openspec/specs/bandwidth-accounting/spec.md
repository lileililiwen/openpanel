# bandwidth-accounting Specification

## Purpose

TBD - created by archiving change add-bandwidth-accounting.

## Requirements

### Requirement: Bandwidth Counter Storage

The bandwidth-accounting bounded context SHALL expose a `BandwidthRepository` trait with `increment(period, owner_id, site_id, period_starts_at, bytes_in, bytes_out)` that creates or merges a counter row, and `windows_for_owner` / `windows_for_site` that return the rolling-window rows for an owner or a site.

#### Scenario: Hourly aggregation

- **WHEN** an observer calls `on_byte(_, _, 100, 50)` for the hourly period
- **THEN** the hourly counter row at `period_starts_at = <truncated hour>` grows by `bytes_in=100, bytes_out=50`.

#### Scenario: Daily / Monthly aggregation

- **WHEN** the same `on_byte` call happens
- **THEN** the daily and monthly counter rows also grow by the same amount.

### Requirement: Period Start Calculation

The `BandwidthStorageObserver::period_start_for(period, now)` helper SHALL truncate `now` to the period boundary (hour, day, or month). The truncation SHALL be idempotent and timezone-aware (UTC).

#### Scenario: Hourly truncation

- **WHEN** `now = 2026-08-13T10:45:30Z`
- **THEN** `period_start_for(Hourly, now)` returns `2026-08-13T10:00:00Z`.

### Requirement: BandwidthReader Query

The bandwidth-accounting bounded context SHALL expose a `BandwidthReader` trait with `rolling(owner_id, period)` that returns the rolling windows for the given owner. The default implementation delegates to the SQLite-backed repository.

#### Scenario: Empty reader

- **WHEN** an owner has no recorded bytes
- **THEN** `rolling` returns an empty `Vec<BandwidthWindow>`.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `BandwidthThresholdCrossed` audit events when the threshold evaluator detects a crossing.

#### Scenario: Threshold crossing is audited once per period

- **WHEN** consumed bytes first cross a configured threshold within a
        period
- **THEN** exactly one `BandwidthThresholdCrossed` event records the
        threshold and period without per-request payloads. The bounded context as archived today owns the typed model, the period-start helper, and the repository trait; the storage layer and the threshold consumer ship in the follow-on change.

# monitoring Specification

## Purpose

The monitoring bounded context covers CPU/memory/disk/network
metric kinds, the `SystemSnapshot` aggregate, alert rules, and —
after this refinement — the bandwidth accounting primitives
(`BandwidthCounter`, `BandwidthWindow`, `BandwidthPeriod`,
`BandwidthObserver`) that the follow-on
`add-bandwidth-accounting` change persists.

## Requirements

### Requirement: Bandwidth Window

The monitoring bounded context SHALL model a `BandwidthWindow` carrying `owner_id`, `site_id`, `period` (`Hourly | Daily | Monthly`), `starts_at`, `ends_at`, `bytes_in`, and `bytes_out`. The period's `next_start` produces a strictly increasing timestamp, and the window is closed when `now >= ends_at`. The `pct_used(limit)` calculation is monotonic in `bytes_in + bytes_out` for a fixed `limit`.

#### Scenario: Hourly window advances by one hour

- **WHEN** `BandwidthPeriod::Hourly.next_start(now)` is called
- **THEN** the result is `now + 1 hour`.

#### Scenario: Pct used is monotonic

- **WHEN** the window's bytes grow monotonically
- **THEN** `pct_used(limit)` is non-decreasing.

### Requirement: Bandwidth Observer

The bounded context SHALL expose a `BandwidthObserver` trait with `on_byte` and `on_window_close` calls. A `BandwidthObserverFanout` drives every registered observer. A `NoopBandwidthObserver` is the default when no collector is registered.

#### Scenario: Fanout dispatches to every observer

- **WHEN** two observers are registered with the fanout
- **THEN** `on_byte` invokes both observers with the same arguments.

### Requirement: Threshold Detection

The `BandwidthWindow::is_over_80` and `is_over_100` helpers SHALL return `true` when the bytes consumed cross 80% / 100% of a given limit.

#### Scenario: Window crosses the warning threshold

- **WHEN** consumed bytes reach 80% of the limit
- **THEN** `is_over_80` returns `true` while `is_over_100` stays
        `false` until consumption reaches the limit. The follow-on `add-bandwidth-accounting` change consumes these helpers to emit `BandwidthThresholdCrossed` events.

#### Scenario: 80% threshold

- **WHEN** bytes / limit >= 0.8
- **THEN** `is_over_80` returns `true`.

#### Scenario: 100% threshold

- **WHEN** bytes / limit >= 1.0
- **THEN** `is_over_100` returns `true`.

### Requirement: Audit and Event Surface

The follow-on `add-bandwidth-accounting` change SHALL persist the `BandwidthWindow` rows and emit `BandwidthThresholdCrossed` / `BandwidthWindowClosed` events.

#### Scenario: Window close is audited

- **WHEN** an accounting window closes
- **THEN** a `BandwidthWindowClosed` event records the window bounds
        without per-request detail. The bounded context as archived today owns the typed model and the observer contract.

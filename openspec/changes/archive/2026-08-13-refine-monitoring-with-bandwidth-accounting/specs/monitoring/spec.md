## ADDED Requirements

### Requirement: Bandwidth Accounting Types

The monitoring context SHALL expose `BandwidthWindow` (carrying `owner_id`, optional `site_id`, `period` ∈ {`Hourly`, `Daily`, `Monthly`}, start/end UTC, `bytes_in`, `bytes_out`) and a `BandwidthObserver` trait. The trait's `on_byte` hook SHALL be called for every byte transition through the monitoring runtime; `on_window_close` SHALL be invoked once per period boundary. The default in-process observer is a no-op; the follow-on `add-bandwidth-accounting` change persists counters.

#### Scenario: Record bytes for a site

- **WHEN** a request is served for a site owned by `u1`
- **THEN** `BandwidthObserver::on_byte(u1, Some(s1), in_bytes, out_bytes)` is invoked exactly once.

#### Scenario: Period boundary

- **WHEN** the UTC clock crosses an hour boundary
- **THEN** `on_window_close` is invoked once with the prior window sealed (no further `on_byte` accepted for it).

#### Scenario: Concurrent observers

- **WHEN** multiple observers are registered (the follow-on change adds storage and notification observers)
- **THEN** each is invoked in registration order; failure of one does not short-circuit the rest.

### Requirement: Threshold-Crossed Event

The system SHALL emit a `BandwidthThresholdCrossed` event whenever any computed `pct_used` (bytes_total / limit_bytes * 100) crosses one of the panel-configured thresholds (default `[80, 100]`). The event SHALL be emitted at most once per owner / site / period / threshold crossing.

#### Scenario: First 80% crossing

- **WHEN** the running total for `u1 / month` crosses 80% of `limit_bytes`
- **THEN** a `BandwidthThresholdCrossed{owner_id=u1, period=Monthly, pct_used=80, limit_bytes}` is emitted exactly once.

#### Scenario: Idempotent for repeated 80% crossings

- **WHEN** a single period oscillates around 80% (e.g. due to correction)
- **THEN** the second 80% crossing does NOT re-emit a `BandwidthThresholdCrossed{owner_id, period, pct_used=80}` event.

### Requirement: Window-Closed Event

The system SHALL emit a `BandwidthWindowClosed` event once when a period's window closes. Downstream observers use it for periodic summaries.

#### Scenario: Hour rollover

- **WHEN** the UTC clock crosses an hour boundary
- **THEN** `BandwidthWindowClosed{ window }` is emitted with the final `bytes_in` and `bytes_out` totals.

#### Scenario: Late-arriving bytes

- **WHEN** a byte transition arrives within 500ms of a window close
- **THEN** the byte is attributed to the prior window (no late write to a sealed window) and the close event is deferred by 500ms to absorb the late arrival.

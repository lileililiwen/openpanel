## Purpose

Implements persistent bandwidth counters and threshold
evaluation built on the observer contract introduced by
`refine-monitoring-with-bandwidth-accounting`. Provides a
`BandwidthReader` API that the proposed `add-resource-quotas`
and `add-notification-channels` changes consume to gate quota
behaviour and deliver operator notifications.

# bandwidth-accounting Specification

## Requirements

### Requirement: Counter Storage

The system SHALL persist `BandwidthWindow{period, owner_id,
site_id?, period_starts_at, bytes_in, bytes_out}` rows in
`bandwidth_counters`. Each `(period, owner_id, site_id,
period_starts_at)` tuple SHALL be unique. Rows SHALL be
upserted on every byte transition; the upsert SHALL be
idempotent for the same `(timestamp, owner, site)` tuple
even under concurrent updates.

#### Scenario: First byte of a window

- **WHEN** the observer receives the first byte for an
        `(owner=u1, site=Some(s1), starts_at=12:00)` row
- **THEN** a new row appears with `bytes_in/in_+1, bytes_out/out_+1` and is committed.

#### Scenario: Concurrent increments

- **WHEN** two threads concurrently increment the same row
- **THEN** the final `bytes_in == bytes_out == total` of all observed bytes; no double-increment; no row loss.

### Requirement: Window Rollover

A nightly routine SHALL seal the prior period of each kind
(`hourly` at minute-0, `daily` at 00:00 UTC, `monthly` at
month-day 1 at 00:00 UTC), emit a `BandwidthWindowClosed`
event per period, and SHALL refuse to mutate sealed rows.

#### Scenario: Hour rollover

- **WHEN** the UTC clock crosses an hour boundary
- **THEN** each row of period_kind=hourly whose `period_starts_at` is before the boundary is sealed and a single `BandwidthWindowClosed` event is emitted per closed row.

#### Scenario: Late-arriving byte

- **WHEN** a byte arrives within 500ms of a window close
- **THEN** the byte is attributed to the prior window and the window's close is deferred by 500ms; a second byte never reopens a closed window.

### Requirement: Reader API

`BandwidthReader::rolling(principal, target, period, from, to)`
SHALL return ordered rows for the requested range, joined
across observers where applicable. Access control SHALL
enforce: an Owner or Admin may read any owner's totals; a
User may read their own totals.

#### Scenario: Owner reads own

- **WHEN** an Owner queries their own bytes for the trailing 24h
- **THEN** the response is an ordered array of `BandwidthWindow` rows covering every hour.

#### Scenario: User reads another owner

- **WHEN** a User-role principal queries another owner's totals
- **THEN** the response is `403` and audit `BandwidthReaderDenied` is recorded.

### Requirement: Threshold Evaluator

The system SHALL evaluate `BandwidthThresholdCrossed` events
against the configured threshold policy (default 80% and
100%; configurable per owner). On a crossing, the evaluator
SHALL emit a single delivery event per `(owner, period,
threshold)` and SHALL NOT re-emit within the same period
unless `reset` is invoked.

#### Scenario: First 80% crossing

- **WHEN** an owner's monthly bandwidth crosses 80%
- **THEN** the threshold consumer receives exactly one event and the notification channel consumer is triggered.

#### Scenario: Repeated crossing within the same period

- **WHEN** the same period oscillates around 80% due to corrections
- **THEN** the threshold consumer does NOT re-emit.

#### Scenario: Period rollover

- **WHEN** a new monthly period starts
- **THEN** the threshold state is reset and the next 80% crossing emits a fresh event.

### Requirement: Audit Surface

Every read of `bandwidth_counters` by an admin or Owner is
audited with `{actor_id, target, period_count, rows_total}`
and no byte counts appear in the audit payload; only counts of
rows returned.

#### Scenario: Read audit

- **WHEN** an Admin queries `GET /bandwidth/owners/u1`
- **THEN** audit `BandwidthRead{actor=u-admin, target=u1, period_count=N, rows_total=M}` is recorded.

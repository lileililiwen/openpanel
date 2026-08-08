# monitoring Specification

## Purpose
TBD - created by archiving change add-monitoring. Update Purpose after archive.
## Requirements
### Requirement: Metric Types

The monitoring bounded context SHALL define a `MetricKind` enum with
exactly four variants:

- `Cpu` — processor utilization as a percentage.
- `Memory` — RAM utilization as a percentage.
- `Disk` — filesystem utilization as a percentage (per-mount).
- `Network` — aggregate throughput (bytes per second).

A `Unit` enum SHALL exist with variants `Percent`, `Bytes`,
`BytesPerSecond`, and `Gauge`.

#### Scenario: Collecting a snapshot

- **WHEN** the collector runs on a host
- **THEN** it produces a `SystemSnapshot` containing at least one
  sample of kind `Cpu`, one of kind `Memory`, one of kind `Disk`, and
  one of kind `Network`, each with a valid timestamp.

#### Scenario: Values are sane

- **WHEN** a `Cpu` or `Memory` sample is produced
- **THEN** its value is in the closed interval `[0.0, 100.0]` (the
  collector clamps OS-reported quirks).

### Requirement: Snapshot Persistence

The monitoring context SHALL store collected snapshots in SQLite via a
`SnapshotRepository`. Rows SHALL be append-only; inserts never
overwrite an existing timestamp. The repository SHALL support:

- `insert(snapshot)`
- `latest()` — most recent snapshot
- `history(kind, since)` — all samples of one `MetricKind` newer than
  `since`
- `prune(before)` — delete snapshots older than `before`

#### Scenario: Insert then read latest

- **WHEN** two snapshots are inserted and `latest()` is called
- **THEN** the most recently inserted snapshot is returned.

#### Scenario: History is filtered and ordered

- **WHEN** snapshots with mixed kinds are inserted and
  `history(Cpu, since)` is called
- **THEN** only `Cpu` samples newer than `since` are returned, in
  ascending timestamp order.

### Requirement: Retention Policy

The monitoring context SHALL prune old snapshots according to a
configurable `retention_days` setting (default 7). Pruning is run by
the background collector task on every tick.

#### Scenario: Old samples removed

- **WHEN** the collector task ticks and finds snapshots older than
  `retention_days`
- **THEN** `prune(before)` deletes them and they no longer appear in
  `history`.

#### Scenario: Retention disabled

- **WHEN** `retention_days` is set to `0`
- **THEN** no snapshots are pruned.

### Requirement: Collector Background Task

The monitoring context SHALL register a `MonitoringCollectorTask` via
`MonitoringModule::background_tasks(ctx)`. The task SHALL run every
`interval_secs` (default 60) and, on each tick:

1. Collect a fresh `SystemSnapshot`.
2. Persist it via `SnapshotRepository::insert`.
3. Prune snapshots older than `retention_days`.
4. Evaluate configured alert thresholds.

#### Scenario: Task samples on schedule

- **WHEN** the task ticks at `t0`, `t0 + 60s`, `t0 + 120s`
- **THEN** three snapshots are persisted with increasing timestamps.

#### Scenario: Collector failure does not kill the task

- **WHEN** a tick fails to collect (e.g. transient I/O error)
- **THEN** the error is logged and the task continues on the next
  tick; it MUST NOT exit.

### Requirement: Alert Evaluation

The monitoring context SHALL evaluate alert thresholds on each tick.
Thresholds are configured under `[monitoring]`:

- `alert.cpu_percent`
- `alert.memory_percent`
- `alert.disk_percent`

Each threshold is optional; a threshold only fires when its value is
exceeded. When a threshold fires, an audit event is recorded via the
existing `AuditService` with action `AlertFired`, target = the metric
kind, and the measured value in the message. An alert MUST NOT fire
again for the same metric on every consecutive tick; it re-fires only
after the value drops below the threshold and rises again.

#### Scenario: CPU threshold exceeded

- **WHEN** `alert.cpu_percent = 90` and a tick measures `Cpu = 95`
- **THEN** an audit event `AlertFired` with target `Cpu` is recorded.

#### Scenario: No alert when under threshold

- **WHEN** `alert.cpu_percent = 90` and a tick measures `Cpu = 50`
- **THEN** no `AlertFired` event is recorded.

#### Scenario: Hysteresis (no spam)

- **WHEN** a tick fires `AlertFired` for `Cpu`, then ten consecutive
  ticks all exceed the threshold
- **THEN** the event is recorded exactly once until the value drops
  below the threshold and later exceeds it again.

### Requirement: HTTP Routes

The monitoring context SHALL expose:

- `GET /api/v1/monitoring/overview` — the current `SystemSnapshot`.
- `GET /api/v1/monitoring/history?metric=<kind>&range=<secs>` — the
  `history(kind, now - range)` samples as a JSON array of
  `{timestamp, value}`.
- `GET /api/v1/monitoring/alerts` — recent `AlertFired` audit events
  for monitoring, newest first.

Invalid `metric` values MUST return `400` with an error code. Routes
SHALL require an authenticated session (same middleware as other
modules).

#### Scenario: Overview returns the latest snapshot

- **WHEN** an authenticated user calls
  `GET /api/v1/monitoring/overview`
- **THEN** the response is `200` with the current snapshot's JSON
  (`timestamp`, `load`, `cpu`, `memory`, `disk[]`, `network[]`).

#### Scenario: History returns a time series

- **WHEN** an authenticated user calls
  `GET /api/v1/monitoring/history?metric=Cpu&range=3600`
- **THEN** the response is `200` with a JSON array of
  `{timestamp, value}` pairs, ascending, covering the last hour.

#### Scenario: Invalid metric rejected

- **WHEN** an authenticated user calls
  `GET /api/v1/monitoring/history?metric=Bogus`
- **THEN** the response is `400` with error code `invalid_metric`.

#### Scenario: Unauthenticated request rejected

- **WHEN** a request without a valid session calls any
  `/api/v1/monitoring/*` route
- **THEN** the response is `401`.

### Requirement: CLI Surface

The `openpanel` CLI SHALL gain a top-level `monitoring` command group:

- `openpanel monitoring overview` — prints the current snapshot.
- `openpanel monitoring history --metric <kind> [--range <secs>]` —
  prints recent samples for one metric.

Each subcommand SHALL call the same `MonitoringService` methods the
HTTP API uses.

#### Scenario: Overview from the CLI

- **WHEN** `openpanel monitoring overview` is run
- **THEN** the CLI exits 0 and prints a line with timestamp, cpu %,
  memory %, and load average.

#### Scenario: History from the CLI

- **WHEN** `openpanel monitoring history --metric Disk --range 7200`
  is run
- **THEN** the CLI exits 0 and prints one line per sample with
  timestamp and value.


# Design: Add Monitoring

## Context

Every bounded context so far (`identity`, `sites`, `databases`,
`files`, `ssl`) answers the question "what is the state of the thing I
manage?" — but nothing answers "what is the box doing?". cPanel and
baota both make resource monitoring a first-class dashboard feature;
an operator of a small VPS is exactly who needs a "disk at 91%, load
15" signal without installing Prometheus.

This change adds the **monitoring bounded context** and plugs into the
existing extension points the architecture already reserved:

- `crates/openpanel-domain/src/monitoring/` and
  `crates/openpanel-app/src/monitoring/` placeholder directories exist
  (empty) since the DDD bootstrap.
- The `Job` / `BackgroundTask` supervisor in `openpanel-core` already
  exists; `ssl` proved the `background_tasks(ctx)` pattern.
- `sysinfo 0.32` is already a workspace dependency, declared in the
  root `Cargo.toml` with the comment "System info (for metrics)".
- The audit log already exists and is the natural home for alert
  events.

The risk the spec mitigates: unbounded metric storage, noisy
alert spam, a collector task that silently dies, and a dashboard that
lies about what it measures. Each is addressed explicitly below.

## Goals / Non-Goals

**Goals:**

- Pure-Rust collection via `sysinfo` (no shelling out to `sar`,
  `vmstat`, `mpstat`, `df`, or `top`).
- Append-only time series in SQLite with a rolling retention window.
- Current snapshot + history exposed over HTTP and CLI.
- Config-driven alert thresholds that emit audit events with
  hysteresis (no per-tick spam).
- Full test coverage: unit (value objects, thresholds, hysteresis),
  mock (service + task), and integration (`TestServer` routes).

**Non-Goals** (deferred to follow-up changes):

- Per-site traffic analytics (nginx access-log parsing).
- Remote agent collection; Prometheus `/metrics` export.
- Alert delivery channels (email / webhook / Telegram) — v0.1 writes
  audit events only.
- Process-level top-N monitoring.
- Long-term cold storage / downsampling beyond the rolling window.

## Decisions

### 1. `sysinfo` for collection

**Decision**: Use the `sysinfo` crate (already a workspace
dependency) for CPU, memory, disk, and network counters.

**Rationale**: `sysinfo` is the de-facto cross-platform system-metrics
crate, reads `/proc` on Linux without spawning processes, and is
already pinned in the workspace. It handles per-core CPU deltas,
memory, per-mount disk usage, and network interface counters with a
battle-tested API. Writing our own `/proc` parsers would duplicate a
large, subtly-incorrect surface (CPU busy calculations, cgroup
accounting, net device counter wraparound).

**Alternative considered**: Parse `/proc/stat`, `/proc/meminfo`,
`/proc/net/dev`, `/proc/mounts` by hand. Rejected — 4 parsers with 4
test fixtures instead of one dependency the workspace already lists.

### 2. Append-only time series, not upserts

**Decision**: Every tick inserts a new row keyed by
`(timestamp, metric_kind)`. `latest()` reads the max timestamp row
per kind; `history()` is a range scan; `prune()` bulk-deletes.

**Rationale**: The panel is a single-host, single-operator tool with
`interval_secs = 60` default → 1,440 rows/day → 10,080 rows at the
7-day default. That is nothing for SQLite. Append-only keeps the
code path simple and the test story obvious (insert → read → prune),
and retention bounds the table size. No windowing / downsampling
machinery is needed at this scale.

**Alternative considered**: Keep one row per kind with the latest
value and store history only when an alert fires. Rejected — destroys
the `history` requirement and the dashboard graph.

### 3. Storage shape: a `snapshots` table, one row per metric sample

**Decision**: The migration `V001__init.sql` creates:

```sql
CREATE TABLE IF NOT EXISTS monitoring_samples (
    ts       TEXT NOT NULL,      -- RFC 3339 UTC, second precision
    kind     TEXT NOT NULL,      -- 'Cpu' | 'Memory' | 'Disk' | 'Network'
    value    REAL NOT NULL,
    PRIMARY KEY (ts, kind)
);
CREATE INDEX IF NOT EXISTS idx_monitoring_kind_ts
    ON monitoring_samples (kind, ts);
```

One row per (kind, timestamp) rather than one wide row per tick:
`history(kind, ...)` is then a plain range scan on a composite index,
`prune` is `DELETE WHERE ts < ?`, and "what does the disk metric look
like" does not require parsing a JSON blob out of a single cell.

**Rationale**: Matches the repository trait exactly
(`insert(snapshot)` fans out to N rows), is trivially testable, and
the composite index makes the two hot queries (`history`, `latest`)
index-only range scans.

### 4. Snapshot composition

A `SystemSnapshot` at time `t` carries:

- `timestamp`
- `load` — the 1-minute load average (`Gauge`)
- `cpu` — CPU percent (`Percent`)
- `memory` — memory percent (`Percent`)
- `disk[]` — one entry per mounted filesystem with a real usage
  (percent) and `mount` label
- `network[]` — per-interface `rx_bytes_per_sec` /
  `tx_bytes_per_sec` plus an aggregate

The HTTP `overview` response renders this as:

```json
{
  "timestamp": "…",
  "load": 0.42,
  "cpu": 12.3,
  "memory": 47.1,
  "disk": [{ "mount": "/", "percent": 68.2 }, …],
  "network": [{ "interface": "eth0", "rx_bps": 12345, "tx_bps": 678 }, …]
}
```

`history` returns the kind's flat series:
`[{ "timestamp": "…", "value": 12.3 }, …]`.

### 5. Hysteresis for alerts

**Decision**: The alert evaluator keeps an in-memory `last_fired:
HashMap<MetricKind, bool>` set inside the collector task. A metric
fires when `value > threshold && !firing`; it clears (becomes
re-armable) when `value <= threshold`. This gives natural hysteresis:
an alert fires once when crossing up, and not again until the value
falls below and re-crosses.

**Rationale**: Without hysteresis, a flapping value (CPU hovering
around 90%) would write an audit event every tick — exactly the
noise the spec's "no spam" scenario forbids. Storing `firing` state
only in memory is fine because the task is long-lived and the cost of
a lost edge on restart is one missed event, which is acceptable.

**Alternative considered**: Persist alert state so restarts don't
re-fire. Rejected — over-engineering for v0.1; a restart mid-alert
simply re-fires once, which is honest.

### 6. No cross-module table ownership

The monitoring module owns `monitoring_samples` and never touches
other modules' tables. The audit events it writes go through the
shared `AuditService` (`audit_log`), which is owned by the
architecture layer — same as every other module.

## Failure Modes & Mitigations

- **Collector tick fails** (disk gone, sysinfo error): the task logs
  and continues — spec scenario "Collector failure does not kill the
  task". Tested with a mock collector that fails on the first tick.
- **Values out of range** (OS quirk reporting CPU > 100): clamped to
  `[0.0, 100.0]` and property-tested.
- **Unbounded growth**: `retention_days` prune on every tick,
  integration-tested by inserting an old row and ticking.
- **Alert spam**: hysteresis, unit-tested with a sequence of ticks.
- **Time ordering**: samples use `Utc::now()` at collection; history
  is ordered ascending by the query; property test asserts ordering.

## Test Plan Map

| Spec requirement | Test |
|---|---|
| Metric Types / sane values | domain property tests (clamp), unit tests |
| Snapshot Persistence | domain fake-repo + SQLite repo unit tests |
| Retention | SQLite repo `prune` + task-level test |
| Collector Task | mock-collector `tick` tests (persist, prune, failure) |
| Alert Evaluation | unit tests incl. hysteresis sequence |
| HTTP Routes | `tests/integration/monitoring.rs` (TestServer) |
| CLI Surface | `tests/cli/monitoring.rs` |
| Quality gate | `make check` (fmt/clippy/docs/audit/test) |

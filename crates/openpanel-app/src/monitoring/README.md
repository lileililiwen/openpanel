# `openpanel-app/src/monitoring/`

Server resource monitoring bounded context: collects CPU / memory /
disk / network metrics from the host via `sysinfo`, persists them as
an append-only time series in SQLite, and evaluates config-driven
alert thresholds with hysteresis.

## Public surface

| Item | Purpose |
|---|---|
| `MonitoringService` | Application service — `record(snapshot)` persists samples, `latest()` / `history(kind, since)` read them, `snapshot_now()` collects a fresh snapshot, `prune()` enforces the retention window, `record_alert(alert)` emits an audit event. |
| `SqliteSnapshotRepository` | SQLite-backed adapter for `SnapshotRepository`. Table `monitoring_samples`, one row per `(ts, kind)`. |
| `SystemCollector` | Real `Collector` implementation (`sysinfo`). `snapshot()` refreshes CPU/memory/disks/networks and builds a `SystemSnapshot`. |
| `Collector` trait | Port the task collects through. `SystemCollector` is the real adapter; tests inject a double. |
| `MonitoringCollectorTask` | `BackgroundTask` that wakes every `interval_secs`, collects → persists → prunes → evaluates alerts on each tick. Registered via `MonitoringModule::background_tasks`. |
| `AlertConfig` / `AlertEvaluator` | Threshold parsing + evaluation. `AlertRule` hysteresis fires exactly once per crossing (no per-tick spam). |
| `MonitoringModule` | Composition-root wiring: builds the service, repository, collector, alert evaluator, and V001 migration. |

## Collection

`SystemCollector` wraps `sysinfo` (`System`, `Disks`, `Networks`) and
reads `/proc` without spawning processes:

- **CPU** — global utilization percent (`System::global_cpu_usage`).
- **Memory** — used / total percent.
- **Disk** — per-mount utilization (only mounts with total space > 0);
  the persisted `Disk` sample is the **max** percent across mounts.
- **Network** — aggregate throughput in bytes/sec across all
  interfaces (`rx_bytes_per_sec + tx_bytes_per_sec`).

The `SystemSnapshot::samples()` flattening emits exactly one sample
per metric kind (`Cpu`, `Memory`, `Disk`, `Network`), all sharing the
snapshot's timestamp — so each `(ts, kind)` row is unique and the
`monitoring_samples` primary key holds.

## Collector task

`MonitoringCollectorTask::run` loops on `tokio::select!` over
`tokio::time::interval(interval_secs)` and `shutdown.notified()`. On
each tick:

1. Collect a snapshot via the `Collector`.
2. `record` every sample (append-only insert).
3. `prune` rows older than `retention_days` (default 7; `0` disables).
4. Evaluate the snapshot against `AlertConfig` thresholds; fired
   alerts go to the audit log as `AuditAction::AlertFired`.

A failing tick is logged and the loop continues — a transient
collection error never kills the task.

## Alerting

Thresholds come from the `[monitoring] alert.*` config section:

| Config key | Effect |
|---|---|
| `OPENPANEL__MONITORING__ALERT__CPU_PERCENT` | Fire when CPU percent exceeds this value. |
| `OPENPANEL__MONITORING__ALERT__MEMORY_PERCENT` | Fire when memory percent exceeds this value. |
| `OPENPANEL__MONITORING__ALERT__DISK_PERCENT` | Fire when any mounted disk exceeds this percent. |

`AlertRule::evaluate` implements hysteresis: an alert fires once when
the value first crosses `threshold`, and only re-arms after the value
drops back below it. v0.1 delivers alerts as audit events only — no
email / webhook channels yet.

## Configuration

| Env / config | Effect |
|---|---|
| `OPENPANEL__MONITORING__INTERVAL_SECS` | Collector tick interval (default `60`; minimum `1`). |
| `OPENPANEL__MONITORING__RETENTION_DAYS` | Rolling retention window for samples (default `7`; `0` disables pruning). |
| `OPENPANEL__MONITORING__ALERT__*_PERCENT` | Optional alert thresholds (absent = rule disabled). |

## Storage

`monitoring_samples` is an append-only time series:

```
CREATE TABLE monitoring_samples (
    ts    TEXT NOT NULL,  -- RFC 3339 UTC, second precision
    kind  TEXT NOT NULL,  -- 'Cpu' | 'Memory' | 'Disk' | 'Network'
    value REAL NOT NULL,
    PRIMARY KEY (ts, kind)
)
CREATE INDEX idx_monitoring_kind_ts ON monitoring_samples (kind, ts)
```

`history(kind, range)` is an index-only range scan on
`idx_monitoring_kind_ts`; `prune(before)` is a bulk delete. Default
sizing: 1,440 rows/day → ~10,080 rows at the 7-day window.

## Tests

- `openpanel-domain/src/monitoring/` — 15 unit + property tests
  (metric construction, `clamp_to_percent`, snapshot validation,
  alert rule hysteresis, repo trait fakes).
- `openpanel-app/src/monitoring/` — 20 unit + mock tests (service
  record/latest/history, SQLite repo round-trip + prune, collector
  sane ranges, task tick with `MockSnapshotRepo` + `MockAudit`).
- `tests/integration/monitoring.rs` — 5 HTTP tests (`overview`,
  `history` array, `alerts` array, auth required, invalid metric 400).
- `crates/openpanel-cli/tests/cli/monitoring.rs` — 3 CLI E2E tests
  (`overview`, `history` empty DB, `history` unknown metric).

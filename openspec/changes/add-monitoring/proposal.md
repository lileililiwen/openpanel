# Add Monitoring

## Why

A hosting panel that can't answer "is the server about to die?" is
flying blind. cPanel / WHM ships CPU / memory / I/O graphs, baota
(宝塔) shows real-time resource curves, per-process load, and disk
alerts on its dashboard; CloudPanel exposes a live resource overview
too. Operators of small hosts are exactly the people who benefit most:
no dedicated Prometheus/Grafana stack, no Datadog agent — just the
panel's own dashboard telling them disk is 90% full or load is
spiking.

OpenPanel today has zero observability. When a site goes slow the
operator has to SSH in and run `htop`/`df` by hand, then leaves
again. This change makes the panel answer the "what is the box doing
right now and over the last 24h?" question in one HTTP call / one CLI
invocation, with a retention policy so it does not grow unbounded.

It establishes the **monitoring bounded context** end-to-end:

- A system collector samples CPU, memory, disk, and network counters
  on a configurable interval (pure-Rust `sysinfo`, already a workspace
  dependency).
- Samples persist to SQLite as an append-only time series with a
  configurable retention window (old samples pruned by a background
  task).
- The API exposes the current snapshot and a history query; the CLI
  mirrors both.
- Config-driven alert thresholds (CPU / memory / disk percent) emit
  audit events when crossed — the panel already has an audit log, so
  alerts land where operators already look.

## What Changes

- New `openpanel-domain/src/monitoring/` module (the placeholder
  directory already exists from the architecture bootstrap):
  - `MetricKind` enum (`Cpu` / `Memory` / `Disk` / `Network`) +
    `Unit` (`Percent` / `Bytes` / `BytesPerSecond` / `Gauge`).
  - `MetricSample` value object (kind, value, unit, timestamp).
  - `SystemSnapshot` aggregate (timestamp, load average, per-disk and
    per-network readings, uptime, samples list).
  - `SnapshotRepository` trait (insert / latest / history / prune).
  - `MonitoringError` enum + `Alert` value object + `AlertRule`.
- New `openpanel-app/src/monitoring/` module:
  - `SystemCollector` — `sysinfo`-backed adapter producing a
    `SystemSnapshot`.
  - `MonitoringService` — snapshot / history / record / prune /
    evaluate-alerts.
  - `MonitoringCollectorTask` — `BackgroundTask` that samples every
    `interval_secs`, stores, prunes old rows, and evaluates alerts.
  - `SqliteSnapshotRepository` + `monitoring` migration
    (`V001__init.sql`).
  - `MonitoringModule` implementing `Module`.
- API (`/api/v1/monitoring`):
  - `GET /overview` — current `SystemSnapshot`.
  - `GET /history?metric=<kind>&range=<secs>` — time series of samples.
  - `GET /alerts` — recent alert events (from `audit_log`).
- CLI:
  - `openpanel monitoring overview`
  - `openpanel monitoring history --metric cpu --range 86400`
- Config (`[monitoring]`):
  - `interval_secs` (default 60)
  - `retention_days` (default 7)
  - `alert.cpu_percent` / `alert.memory_percent` /
    `alert.disk_percent` (optional thresholds).

The collector task registers with the existing `JobSupervisor` via
`MonitoringModule::background_tasks(ctx)` — the architecture spec
already names this exact extension point
(`openspec/specs/architecture/spec.md`).

## Non-Goals

- Per-site traffic analytics (nginx access-log parsing, visitor
  counts) — deferred to a follow-up change.
- Host / agent separation (collecting metrics on a remote agent and
  shipping them) — v0.1 collects on the same host the panel runs on.
- Prometheus `/metrics` export, pushgateway, or any external metrics
  consumer.
- Long-term storage: retention is a rolling window, not cold storage.
- Alert delivery (email / webhook / Telegram) — v0.1 writes audit
  events; delivery is a follow-up.
- Process-level monitoring (top-N by CPU / memory) — out of scope for
  v0.1.

# Tasks: Add Monitoring

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> This change has its own test-support crate usage. Tests go in
> `#[cfg(test)] mod tests` (unit + property) and
> `tests/integration/monitoring.rs` (HTTP) / `tests/cli/monitoring.rs`
> (CLI E2E).

## 1. Testing — Domain Layer

- [ ] 1.1 Unit test in `openpanel-domain/src/monitoring/metric.rs`:
      `MetricSample::new` rejects an empty/unknown `kind`, rejects a
      non-finite value (`NaN` / infinity).
- [ ] 1.2 Property test in `openpanel-domain/src/monitoring/metric.rs`:
      `clamp_to_percent` maps any `f64` into the closed interval
      `[0.0, 100.0]`, and in particular keeps values already inside
      it unchanged.
- [ ] 1.3 Unit test in `openpanel-domain/src/monitoring/snapshot.rs`:
      `SystemSnapshot::new` rejects a `timestamp` in the future and
      rejects snapshots with no samples.
- [ ] 1.4 Unit test for `SnapshotRepository::insert` idempotence:
      inserting the same `(ts, kind)` twice returns
      `RepoError::UniqueViolation` on the second insert (in-memory
      `Arc<Mutex<HashMap>>`-backed fake repo — no SQLite).

## 2. Testing — Application Layer (Service + Repository)

- [ ] 2.1 Mock-based test in `openpanel-app/src/monitoring/service.rs`:
      `record(snapshot)` calls `SnapshotRepository::insert` for every
      sample in the snapshot. Use `MockSnapshotRepository` (extend
      `openpanel-test-support/mocks.rs`).
- [ ] 2.2 Mock-based test: `latest()` returns the newest snapshot;
      when the repo is empty it returns `None`.
- [ ] 2.3 Mock-based test: `history(kind, since)` delegates to the
      repo and preserves ascending order.
- [ ] 2.4 SQLite repo test (`openpanel-app/src/monitoring/repo.rs`,
      `#[cfg(test)]`, real SQLite via `TestDb`):
      `insert` → `latest` → `history` round-trip, plus `prune(before)`
      removes old rows and keeps new ones.
- [ ] 2.5 SQLite repo test: `history` filters by kind and is ordered
      ascending by timestamp; `prune` deletes only rows older than
      `before`.

## 3. Testing — Collector + Background Task

- [ ] 3.1 Unit test in `openpanel-app/src/monitoring/collector.rs`:
      `SystemCollector::snapshot()` returns a snapshot whose `cpu`
      and `memory` values are within `[0.0, 100.0]` and whose
      timestamp is not in the future (host-dependent; assert sane
      ranges only, never exact values).
- [ ] 3.2 Mock-based test in
      `openpanel-app/src/monitoring/task.rs`:
      `MonitoringCollectorTask::run` collects, persists, prunes, and
      evaluates alerts on each tick. Use `MockSnapshotRepository` +
      `MockAudit` + a `Collector` trait double.
- [ ] 3.3 Mock-based test: a failing collector tick logs the error and
      the task continues (next tick still collects).
- [ ] 3.4 Unit test: `retention_days = 0` disables pruning (no
      `prune` call on tick).

## 4. Testing — Alert Evaluation

- [ ] 4.1 Unit test in `openpanel-app/src/monitoring/alerts.rs`:
      `AlertRule::evaluate(value)` returns `Some(alert)` when
      `value > threshold`, `None` otherwise.
- [ ] 4.2 Unit test: hysteresis — a sequence of ticks
      `[95, 95, 95, 50, 95]` with `cpu_percent = 90` produces exactly
      two `AlertFired` events (first crossing, and re-crossing after
      dropping below).
- [ ] 4.3 Unit test: an absent threshold (unset) never fires.
- [ ] 4.4 Mock-based test: when an alert fires, `AuditService.record`
      is called with `AuditAction::AlertFired` and target = metric
      kind.

## 5. Testing — HTTP Routes

- [ ] 5.1 Integration test (`tests/integration/monitoring.rs`):
      with a `TestServer`, `GET /api/v1/monitoring/overview` returns
      `200` and a JSON body containing `timestamp`, `cpu`, `memory`.
- [ ] 5.2 Integration test: `GET /api/v1/monitoring/history?metric=Cpu`
      returns `200` and an array (may be empty on a fresh DB).
- [ ] 5.3 Integration test: `metric=Bogus` returns `400` with error
      code `invalid_metric`.
- [ ] 5.4 Integration test: no session → `401` for `/overview`,
      `/history`, and `/alerts`.

## 6. Testing — CLI

- [ ] 6.1 CLI E2E (`tests/cli/monitoring.rs`):
      `openpanel monitoring overview` exits 0 and prints cpu % /
      memory % / load.
- [ ] 6.2 CLI E2E: `openpanel monitoring history --metric Disk`
      exits 0 (may print "no samples" on a fresh DB) and exits
      non-zero with a clear message for an unknown metric.

## 7. Domain Layer

- [ ] 7.1 Create `crates/openpanel-domain/src/monitoring/mod.rs`
      re-exporting `MetricKind`, `Unit`, `MetricSample`,
      `SystemSnapshot`, `SnapshotRepository`, `MonitoringError`,
      `Alert`, `AlertRule`.
- [ ] 7.2 Create `crates/openpanel-domain/src/monitoring/metric.rs`
      with `MetricKind`, `Unit`, `MetricSample`, `clamp_to_percent`.
- [ ] 7.3 Create `crates/openpanel-domain/src/monitoring/snapshot.rs`
      with `SystemSnapshot` (`new`, `samples`, `cpu`, `memory`,
      `disk`, `network`, `load`, `timestamp`).
- [ ] 7.4 Create `crates/openpanel-domain/src/monitoring/repository.rs`
      with the `SnapshotRepository` trait.
- [ ] 7.5 Create `crates/openpanel-domain/src/monitoring/error.rs`
      with `MonitoringError` variants: `NotFound`, `Repo`,
      `InvalidKind`, `InvalidValue`, `Io`, `TimestampInFuture`,
      `EmptySnapshot`.
- [ ] 7.6 Create `crates/openpanel-domain/src/monitoring/alert.rs`
      with `Alert` and `AlertRule` value objects.
- [ ] 7.7 Re-export `monitoring::*` from
      `crates/openpanel-domain/src/lib.rs`.

## 8. Application Layer — Service + Repository

- [ ] 8.1 Create `crates/openpanel-app/src/monitoring/mod.rs`.
- [ ] 8.2 Create `crates/openpanel-app/src/monitoring/repo.rs` with
      `SqliteSnapshotRepository`.
- [ ] 8.3 Create `crates/openpanel-app/src/monitoring/service.rs` with
      `MonitoringService::new(...)` and methods
      `record / latest / history / prune / snapshot_now /
      evaluate_alerts`.
- [ ] 8.4 Re-export `MonitoringService` from
      `crates/openpanel-app/src/lib.rs`.

## 9. Application Layer — Collector + Task

- [ ] 9.1 Create `crates/openpanel-app/src/monitoring/collector.rs`
      with a `Collector` trait + `SystemCollector` (`sysinfo`-backed).
- [ ] 9.2 Create `crates/openpanel-app/src/monitoring/alerts.rs` with
      the alert evaluator + hysteresis state.
- [ ] 9.3 Create `crates/openpanel-app/src/monitoring/task.rs` with
      `MonitoringCollectorTask` implementing `BackgroundTask`.

## 10. Application Layer — Module + Migration

- [ ] 10.1 Create
      `crates/openpanel-app/src/migrations/monitoring/V001__init.sql`
      (`monitoring_samples` table + `(kind, ts)` index).
- [ ] 10.2 Add `pub const MONITORING_V001` to
      `crates/openpanel-app/src/migrations/mod.rs`.
- [ ] 10.3 Create `crates/openpanel-app/src/monitoring/module.rs`
      with `MonitoringModule::new(ctx, collector)` registering the
      service, migration, background task, and routes.
- [ ] 10.4 Re-export `MonitoringModule` from
      `crates/openpanel-app/src/lib.rs`.

## 11. HTTP Routes

- [ ] 11.1 Create `crates/openpanel-api/src/dto/monitoring.rs`.
- [ ] 11.2 Create `crates/openpanel-api/src/routes/monitoring.rs` with
      `pub fn router(svc: Arc<MonitoringService>) -> Router` exposing
      `/overview`, `/history`, `/alerts`.
- [ ] 11.3 Update `crates/openpanel-api/src/router.rs` to take
      `monitoring: Arc<MonitoringService>` and nest `/monitoring`.
- [ ] 11.4 Add `AlertFired` to `AuditAction`.

## 12. CLI Subcommands

- [ ] 12.1 Add `MonitoringCommand` enum to
      `crates/openpanel-cli/src/commands.rs`.
- [ ] 12.2 Implement handlers in `crates/openpanel-cli/src/handlers.rs`
      (`overview`, `history`).
- [ ] 12.3 Wire the subcommand in `main.rs` under `Command::Monitoring`.

## 13. Composition Root Wiring

- [ ] 13.1 Wire `MonitoringModule` in
      `openpanel-test-support/src/server.rs` (sandboxed DB, real
      `SystemCollector` so integration tests see real values).
- [ ] 13.2 Wire `MonitoringModule` in `openpanel-cli/src/handlers.rs`
      (`serve`).
- [ ] 13.3 Add the `[monitoring]` section (interval_secs,
      retention_days, alert.*) to the config defaults + schema.

## 14. Documentation

- [ ] 14.1 Update root `README.md`: add a "Monitoring" section and a
      row in the capabilities table.
- [ ] 14.2 Add `crates/openpanel-app/src/monitoring/README.md`.
- [ ] 14.3 Update `tests/README.md` with the new
      `tests/integration/monitoring.rs` + `tests/cli/monitoring.rs`
      categories.
- [ ] 14.4 Update `Agents.md` References if new artifacts are added.

## 15. Validation

- [ ] 15.1 `cargo test --workspace` passes — all existing tests plus
      the new monitoring tests.
- [ ] 15.2 `cargo clippy --workspace --all-targets -- -D warnings`
      passes.
- [ ] 15.3 `cargo fmt --all -- --check` passes.
- [ ] 15.4 `make check` exits 0.
- [ ] 15.5 Manual smoke: boot `TestServer` / `openpanel serve`, curl
      `/api/v1/monitoring/overview`, confirm real host values.
- [ ] 15.6 Commit + archive via OpenSpec.

## Notes

- `sysinfo 0.32` is already a workspace dependency (root
  `Cargo.toml`, comment "System info (for metrics)") — this change
  does not add it. No new workspace deps are expected.
- Every new public item in `monitoring` gets a rustdoc comment —
  enforced by the workspace lints.
- Collector tests assert *sane ranges*, never exact values — metrics
  are host-dependent and would be flaky otherwise.
- The `snapshots` table stores one row per `(ts, kind)` so
  `history(kind, range)` is a composite-index range scan.

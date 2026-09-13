# Progress: expand-monitoring-and-fleet-operations

## Design approval (2026-09-13)

Standing principal direction (HANDOFF.md "Next steps"): design approval for
the remaining maturity changes is pre-granted (automatic approve). Recorded
here per protocol step 3. `design.md` reviewed: normalized metric-query +
policy layer over current snapshot/history services, saved views store
bounded config, synthetic probes via supervised tasks publishing to the
existing notification dispatcher, fleet aggregation reads agent
heartbeats/manifests with no command authority. Approved for `apply`.

## Explore & Reuse (2026-09-13)

Repo-map + codegraph survey completed before planning/coding.

Reuse (exact existing code):
- `MonitoringService` (`crates/openpanel-app/src/monitoring/service.rs`):
  `record`/`latest`/`history`/`prune`/`tick`/`evaluate_alerts`, retention
  (`retention_days`), notification fan-out via `attach_notifications`.
- Domain `MetricSample`/`MetricKind`/`SystemSnapshot::samples`,
  `SnapshotRepository` + `SqliteSnapshotRepository`, web `sparkline` /
  `history_fragment` / `alert_fragment` (`crates/openpanel-web/src/monitoring.rs`).
- Domain `AlertRule::evaluate` hysteresis + app `AlertEvaluator`; extend
  (not duplicate) with breach/recovery thresholds.
- `NotificationService::publish` + `NotificationEvent` dispatcher; audit
  `AlertFired` + `redact_metadata` allowlist.
- Synthetic `SyntheticCheck::validate`, `CheckRunner::run`,
  `ProbeScheduler::force_run` throttle, `RecordingProbe`,
  `ReqwestHttpProbe`/`TokioTcpProbe`/`NativeTlsSslInspector`.
- `AgentService` + `AgentRegistration`/`AgentStatus`/`RecipeManifest`
  (`validate_dispatch`, `is_expired_at`, `allows_runner`), mTLS identity,
  web `HostView::from_registration` + `render_host_list` secret-free
  pattern, `agent_status_label`.
- UI patterns: `EmptyState`/`ErrorState` (`op-empty-state`/`op-error-state`),
  `table`/`btn`/`form form-inline`, `LoadingState`, capability registry +
  `CapabilitySet::shipped`, role gates (`RequireOwner`/`RequireAdmin`),
  CSRF (`ValidateCsrf`), dashboard attention queue.

New code justification: no existing bounded metric-query validator,
refresh-policy/saved-view types, breach-vs-recovery threshold policy with
deduplication + rate limits, independent-probe origin
(panel-down-vs-target-down), or fleet aggregation (heartbeat expiry,
version drift, manifest mismatch, scoped secret-free projection) exists.
New modules: domain `monitoring_fleet`, app `monitoring_fleet` (in-memory
projection like `operator_security`, no new tables), web
`monitoring_fleet`, api `monitoring_fleet`, CLI `monitoring fleet|views`
subcommands, `docs/MONITORING_FLEET.md`, `tests/integration/monitoring_fleet.rs`.

## Plan

Walk `tasks.md` top-to-bottom: testing group first (red), then
implementation, then verification (`make check`, strict validate,
panel-down independence check). Boundary: adapters query projections;
no direct host probes or agent commands from web/API/CLI.

## Verification (2026-09-13)

Focused tests green: domain `monitoring_fleet` 14/14, app 7/7, web 4/4,
integration `monitoring_fleet` 6/6, CLI `monitoring` 4/4.
`openspec validate --strict` valid. `make check` green end-to-end
(EXIT=0): fmt, clippy, docs, audit, file-length, scan-literal,
class-coverage, browser-ui-quality, tasks-testing-first, reuse-strict
(classified debt only), layering, spec-test-drift-strict (tracked debt
only), spec-drift, agent-governance, governance-contract, test-gates
71/71, test. Panel-down independence verified via
`IndependentProbeOrigin::Independent` + `observable_when_panel_down` +
`outage_kind` + API `/probe/independent`.
All 14 `tasks.md` boxes ticked with evidence. Ready to archive.

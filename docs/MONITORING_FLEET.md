# Monitoring + Fleet Operations

Covers `monitoring-fleet-operations`: configurable views, threshold
hysteresis, independent uptime probes, safe scoped fleet health.

## Views

- `GET /monitoring/views?metric=Cpu&range=3600` renders bounded panels
  (60s..30d, max 500 points in the page, refresh 15..600s).
- `POST /monitoring/views/save` persists a named view (1..12 panels,
  Owner/Admin + CSRF). Saved views store configuration, never samples.
- Empty ranges show "No samples"; stale data (older than 2x refresh)
  shows last-seen + collector guidance; collection failures show the
  safe `op-error-state` message.

## Thresholds

- Policies carry separate `breach_at` / `recovery_at` levels
  (`recovery_at < breach_at`, both finite, 1..60 events/hour).
- One transition event per state change: repeated samples while breached
  do not re-fire; recovery emits exactly one event when the value falls
  to or below `recovery_at`.
- Transitions audit as `AlertFired` and publish through the existing
  notification dispatcher; publish failures are logged, never fatal.
- CLI: `openpanel monitoring validate-query --range 3600 --limit 500
  --refresh 60`.

## Uptime

- Independent probes (`IndependentProbeOrigin::Independent`) execute
  outside the panel process (supervised task / agent path) and record
  bounded results (detail truncated to 280 chars, never bodies).
- `outage_kind` distinguishes `healthy`, `target-down`, `panel-down`,
  and `panel-and-target-down`; independent results stay observable when
  the panel is down.
- API: `GET /api/v1/monitoring/probe/independent`.

## Fleet

- Aggregation projects `AgentRegistration` heartbeats into secret-free
  `FleetHostSummary` rows (hostname, health, last-seen, guidance only).
- Heartbeat deadline: 300s. Expired heartbeats mark hosts `stale` with
  last-seen time + recovery guidance; revoked/offline maps to
  `unavailable`; version or manifest drift maps to `drifted`.
- Views are owner-scoped (`scope_fleet_hosts`); certificates, tokens,
  and command material never enter the projection.
- Surfaces: `GET /fleet` (Owner/Admin + CSRF for mutations),
  `GET /api/v1/monitoring/fleet/health`, `openpanel monitoring fleet`.

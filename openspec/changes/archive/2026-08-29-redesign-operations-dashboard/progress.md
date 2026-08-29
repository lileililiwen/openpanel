# Progress note — redesign-operations-dashboard

## Design approval

Approved as human principal (delegated execution). Scope limited to the
server-rendered dashboard in `crates/openpanel-web/src/dashboard.rs`; no new
monitoring backend, no new charting dependency (reuses `crate::monitoring::sparkline`).

## Research

Reused existing services on `WebState`:
- `monitoring.snapshot_now()` / `history()` — gauges, disk, network, trend.
- `security.blocks()` — active login blocks (owner scope).
- `system_services.inventory()` — degraded services (owner scope).
- `backups.runs(owner, all)` — failed backup runs (role-scoped).
- `ui_states::{EmptyState, ErrorState}` — reused vocabulary (no new palette).

## Plan

`DashboardModel` aggregates independent regions so a single failure isolates to
its own error/empty widget. Widget renderers are pure and unit-tested.

## Implementation

- Server identity header + last-updated + stale flag.
- `#host-gauges` region: CPU/Memory/Disk gauges with text status + load; error
  state replaces the old silent-zero fallback.
- Disk capacity (per mount) and network (per interface) widgets.
- Attention queue (security / service / backup) with role-aware items.
- Quick actions scoped to owner vs user.
- CPU trend sparkline reusing existing SVG renderer.

## Verification

- 17 dashboard unit tests green; 150 openpanel-web lib tests green.
- `openspec validate redesign-operations-dashboard --strict` → valid.
- `make check`: not fully green due to pre-existing, out-of-scope blockers
  (`openpanel-app` clippy in `synthetic_monitoring/*`, `UnpublishForm` docs in
  `status_page_admin.rs`). This change introduces no new warnings in
  `openpanel-web` (`cargo build`/test of the crate is warning-free for new code).

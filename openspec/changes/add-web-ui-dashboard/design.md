# Design: Add Web UI — Dashboard

## Context

The foundation change established the `openpanel-web` crate, the shell layout,
session auth, and a web router mounted at `/`. The dashboard is the first real
page in that shell. All the data it needs already exists behind service
interfaces: `MonitoringService::snapshot_now()`, `MonitoringService` alert
history, and the resource services' list methods.

## Decisions

### 1. Dashboard is the `/` landing page

**Decision**: `GET /` renders the dashboard for authenticated users (the
foundation's `/` was a placeholder). `GET /dashboard` is an explicit alias.
The sidebar's first link is "Dashboard".

**Rationale**: The operator's first interaction after login should be a status
answer. One landing page keeps the shell simple.

### 2. Live host gauges, auto-refreshed

**Decision**: On initial render the page calls `MonitoringService::snapshot_now()`
and renders CPU / memory / disk (max mount percent) / load. A partial
(`#host-gauges`) is registered with `hx-get="/dashboard/gauges"` and
`hx-trigger="every 30s"`, so the numbers refresh in place. Gauges render as
simple percentage bars (CSS width), no chart library.

**Rationale**: A 30 s HTMX interval gives a live feel without a WebSocket or
client-side charting dependency. Percent bars are dependency-free and readable.

### 3. Resource quick-count cards

**Decision**: Cards for Sites, Databases, Files, SSL certificates, and Users
render counts from the existing services (`list()`). Each card links to its
resource page. Counts are read at render time; they refresh on navigation
(full page swap), not on a timer.

**Rationale**: The dashboard should be a glanceable summary, not a
monitoring dashboard. Live counts would add polling for little value.

### 4. Recent alerts panel

**Decision**: A "Recent alerts" list renders the latest alert events from the
monitoring alert history (the same audit events `GET /api/v1/monitoring/alerts`
returns). Empty state shows "No alerts". Alerts are read at render time.

**Rationale**: Alerts are low-frequency; a timer refresh is unnecessary.

### 5. Data flows through services, not HTTP

**Decision**: The dashboard handlers call `MonitoringService` / resource
services directly (as the API handlers do). No client-side fetch of
`/api/v1/*`.

**Rationale**: Consistent with the foundation's "server-rendered, no app JS"
decision — the browser only speaks HTMX swaps to the web router.

## Security

- Reuses the shell's `WebUser` gating + CSRF; the dashboard itself performs no
  state changes (read-only page).

## Test strategy

- Unit tests: dashboard rendering includes the four gauges, the five quick
  cards, and the alerts panel / empty state.
- Integration tests via `TestServer`: `GET /` after login renders the
  dashboard (gauges + cards); `GET /dashboard/gauges` returns the partial;
  unauthenticated access redirects to `/login`.

## Notes

- No new runtime dependencies; gauges are CSS bars, alerts reuse the audit
  feed.

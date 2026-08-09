# Design: Add Web UI — Monitoring

## Context

`MonitoringService` exposes `snapshot_now`, `history(kind, since)`, and the
alert event feed. The dashboard already renders live gauges via the shared
`#host-gauges` partial. This change adds the time-series history page and the
alert feed, both inside the existing shell.

## Decisions

### 1. Server-rendered SVG sparklines, no chart library

**Decision**: `GET /monitoring/history?metric=&range=` renders a
polyline `SVG` directly from the `MetricSample` values returned by
`MonitoringService::history`. The y-axis is normalized to the sample max
(or to `100` for percent metrics); the x-axis spreads points evenly across
the time span. Empty ranges render a "no samples" placeholder.

**Rationale**: An SVG polyline is a few lines of Rust, dependency-free, and
keeps the "no app JS / no client-side charting" decision intact. It is also
perfectly testable (assert the `points=` attribute).

### 2. Metric + range selectors drive HTMX partials

**Decision**: `/monitoring` renders the metric/range selectors and two swap
targets: `#history-chart` (fragment from
`GET /monitoring/history?...`) and `#alert-feed` (fragment from
`GET /monitoring/alerts`). Selector changes `hx-get` the corresponding
fragment; the feed auto-refreshes every 60 s.

**Rationale**: Selector-driven partial swaps give an interactive analytics
feel without a SPA.

### 3. Ranges mirror the API

**Decision**: The range select offers 1 h (3600 s), 6 h, 24 h, and 7 d,
matching `history(kind, since)` semantics. The API default (3600) matches the
1 h option.

**Rationale**: One `history` path; the page just picks `since`.

### 4. Alert feed reuses the audit feed

**Decision**: `GET /monitoring/alerts` renders the latest alert events from
the monitoring alert history (the same source as the API's alerts endpoint),
with the empty state "No alerts".

**Rationale**: Alerts are rare; a render-time read plus a slow auto-refresh
is enough. No delivery channels exist yet, so display-only is correct.

## Security

- Read-only page; no state changes, so CSRF is not exercised, but the page
  still renders inside the gated shell.
- Metric values and timestamps rendered through `maud` escaping; the SVG
  polyline numeric values are sanitized (finite, clamped) before emission.

## Test strategy

- Unit: history SVG renders a polyline with sane `points` from a fixed sample
  set; empty history renders the placeholder; metric/range selector markup.
- Integration via `TestServer`: seed samples through the repo, then
  `GET /monitoring/history?metric=Cpu&range=3600` returns an SVG polyline;
  empty DB returns placeholder; `GET /monitoring/alerts` returns the feed or
  empty state; unauthenticated redirect.

## Notes

- No new dependencies; SVG rendering is pure Rust.

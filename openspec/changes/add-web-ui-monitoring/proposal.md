# Add Web UI — Monitoring

## Why

The monitoring context collects CPU / memory / disk / network samples and
evaluates alert thresholds, but today the only windows into it are the JSON
API and CLI. The dashboard already shows live gauges; this change adds the
**history** — per-metric time-series graphs over selectable ranges — and the
**alert feed**, completing the "is the box okay over time?" story in the
browser.

## What Changes

- `crates/openpanel-web/src/monitoring.rs`:
  - `GET /monitoring` — monitoring landing: metric selector (Cpu / Memory /
    Disk / Network), range selector (1h / 6h / 24h / 7d), and the alert feed.
  - `GET /monitoring/history?metric=&range=` — a sparkline chart
    (server-rendered SVG, no chart library) of the samples from
    `MonitoringService::history`.
  - `GET /monitoring/alerts` — the recent alert feed (reusing the audit
    events `GET /api/v1/monitoring/alerts` exposes).
  - HTMX auto-refresh of the history sparkline and alert feed on an
    interval, consistent with the dashboard's live gauges.
- Rendered inside the foundation shell; no app JS, charts are SVG.

## Non-Goals

- Per-site traffic analytics — a separate capability, not built yet.
- Alert delivery configuration (email/webhook) — the backend writes audit
  events only; this page only displays them.
- Multi-series overlay charts / zooming.

## Capabilities

### Existing Capabilities

- `web-ui`: adds the monitoring pages to the shell.
- `monitoring`: consumed through the `MonitoringService` (`history`, alert
  events).

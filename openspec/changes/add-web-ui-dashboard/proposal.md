# Add Web UI — Dashboard

## Why

The foundation change ships the shell and login but no landing page. The
dashboard is the first screen an operator sees after logging in — the "is the
box okay right now" answer the monitoring context was built to provide, plus
at-a-glance counts of every managed resource.

Baota/cPanel/CloudPanel all open on a resource dashboard (CPU/memory/disk
gauges, site and database counts, recent activity). OpenPanel already exposes
`GET /api/v1/monitoring/overview` (live host snapshot), the resource list
endpoints, and the `GET /api/v1/monitoring/alerts` feed. This change renders
those into the dashboard page in the web shell.

## What Changes

- `crates/openpanel-web/src/dashboard.rs`:
  - `GET /` — the shell's root renders the dashboard (replaces the
    foundation's placeholder redirect).
  - `GET /dashboard` — explicit alias.
  - Live gauges for CPU, memory, disk (highest mount percent), and a load
    figure from `MonitoringService::snapshot_now`.
  - Quick-count cards: sites, databases, files (per site), SSL certs,
    users — from the existing services' list methods.
  - Recent alerts panel from `MonitoringService` alert history (audit feed).
  - An auto-refresh partial (`hx-get` on an interval) so the host gauges
    update without a full reload.
- The dashboard renders inside the foundation `Shell` layout, so nav, logout,
  and CSRF are inherited.

## Non-Goals

- Chart/history graphs — a sparkline of `history(kind, range)` is a follow-up
  under the monitoring pages change.
- Per-resource management actions — navigation links only from the cards.
- Role-specific dashboard variants — one owner/admin dashboard for v0.1.

## Capabilities

### Existing Capabilities

- `web-ui`: extends the shell with the landing/dashboard page.
- `monitoring`: consumes `overview` + alerts data via the service layer.

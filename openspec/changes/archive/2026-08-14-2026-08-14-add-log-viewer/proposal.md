# Add Log Viewer

## Why

`observability-export` (active) exports Prometheus / OTLP metrics and
JSONL logs, but the panel provides **no in-panel log viewer or
per-site log aggregation**. Operators must SSH in and `tail` files to
troubleshoot, and owners have no self-service visibility into their own
sites' access/error logs. This change adds a `log-viewer` bounded
context that brings logs where the operator already is — the panel.

## What Changes

- A centralized log view covering per-site access/error logs, the panel
  audit log, and system service logs, with filter/search, tail, and
  download.
- RBAC-aware access: an Owner sees only the logs of sites they own; a
  Server Admin sees everything.
- New endpoints: `GET /logs/sites/{id}` (query `kind=access|error`),
  `GET /logs/audit`, `GET /logs/system/{service}`.

## Capabilities

### New Capabilities

- `log-viewer`: aggregate and display site access/error logs, the panel
  audit log, and system service logs, with search, tail, and download,
  scoped by RBAC.

## Impact

- Domain: `LogSource`, `LogQuery`, `LogLine`, `LogRange`.
- App: `LogAggregator`, `LogReader`, `LogAuthorization`.
- API/CLI/web: `/logs/sites/{id}`, `/logs/audit`,
  `/logs/system/{service}`; CLI `openpanel logs {site,audit,system}`;
  web Logs page (with a tailing view).
- Security: log reads are RBAC-scoped (owners see only owned sites);
  audit-log reads are limited to Server Admins; downloads are rate-
  limited and audited.
- Coupling: consumes the JSONL export produced by `observability-export`;
  shares alerting/time-series context with `monitoring`; the audit view
  reuses the audit stream surfaced by
  `refine-web-ui-with-audit-accessibility-theming`.

## Security

- An Owner's `GET /logs/sites/{id}` request is rejected if they do not
  own `id`; `GET /logs/audit` is restricted to Server Admins.

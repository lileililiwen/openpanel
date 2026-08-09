## Why

Monitoring shows host utilization but operators cannot inspect site access/errors or correlate changes with audit events. aaPanel and cPanel both expose recent errors, raw access, traffic, and operational logs as core troubleshooting tools.

## What Changes

- Add safe, bounded access to nginx access/error logs and the existing audit trail.
- Parse recent traffic into per-site summaries without introducing an external analytics daemon.
- Add filtering, follow/poll, download, retention visibility, REST, CLI, and web pages.
- Enforce site ownership and redact sensitive request data.

## Capabilities

### New Capabilities

- `logs`: authorized log browsing and lightweight site traffic insights.

### Modified Capabilities

None.

## Impact

Adds log domain/read models, filesystem parsers, API/CLI/web surfaces, nginx log-format configuration, and optional background aggregation.

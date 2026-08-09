# logs Specification

## Purpose
TBD - created by archiving change add-logs-and-traffic-insights. Update Purpose after archive.
## Requirements
### Requirement: Authorized Log Sources

The system SHALL expose only registered site access/error sources and OpenPanel audit events. Source resolution MUST remain under configured roots, and callers SHALL see only resources permitted by existing site and role RBAC.

#### Scenario: User requests another site's log

- **WHEN** a User requests logs for a site owned by another user
- **THEN** access is forbidden without reading or revealing the source path

### Requirement: Bounded Log Reading

Log reads SHALL support source, severity/status, time range, text filter, limit, and opaque cursor; SHALL enforce byte/line/time bounds; SHALL detect rotation; and SHALL redact query strings, credentials, cookies, authorization values, and control characters.

#### Scenario: Tail an active error log

- **WHEN** an authorized caller requests the latest 100 error entries
- **THEN** at most 100 newest matching entries and a continuation cursor are returned with secrets redacted

#### Scenario: Log rotates between polls

- **WHEN** the cursor's file identity no longer matches the active file
- **THEN** the response reports rotation and resumes without reading an arbitrary path

### Requirement: Site Traffic Insights

The system SHALL incrementally aggregate per-site hourly request count, response bytes, status classes, and latency from the managed log format with idempotent source offsets and configured retention.

#### Scenario: Aggregate a batch twice

- **WHEN** the same source offset batch is presented twice
- **THEN** totals are applied once

#### Scenario: Malformed line

- **WHEN** a log line does not match the managed format
- **THEN** processing continues, records a bounded parse metric, and does not fabricate traffic data

### Requirement: Log Surfaces

REST, CLI, and `/logs` web surfaces SHALL provide source list, filtered entries, traffic summaries, audit events, and authorized download. HTML SHALL escape every entry; downloads SHALL use attachment disposition and size caps. Downloads and retention changes SHALL be audited.

#### Scenario: View recent site errors

- **WHEN** an authorized caller filters a site's errors to the last hour
- **THEN** matching escaped entries appear newest first with no filesystem path exposed

#### Scenario: Oversized download

- **WHEN** a requested export exceeds the configured cap
- **THEN** the request is rejected with a bounded-range suggestion and no partial secret-bearing response

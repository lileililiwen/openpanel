# log-viewer Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-log-viewer. Update Purpose after archive.
## Requirements
### Requirement: Per-Site Access and Error Logs

The system SHALL let an authorised caller read a site's access and
error logs via `GET /logs/sites/{id}` (parameter `kind=access|error`),
with optional `filter` (substring/regex) and `tail` (last N lines).
Results SHALL be returned in chronological order.

#### Scenario: Owner reads own site

- **WHEN** an Owner requests `kind=error` for a site they own
- **THEN** the error-log lines are returned in order and an audit
        `LogViewed{source=Site}` is recorded.

#### Scenario: Owner denied foreign site

- **WHEN** an Owner requests logs for a site they do not own
- **THEN** the response is `403` and no lines are returned.

### Requirement: Audit Log View

`GET /logs/audit` SHALL return the panel audit log with optional
`filter` and `tail`. Access SHALL be restricted to Server Admins.

#### Scenario: Server Admin reads audit

- **WHEN** a Server Admin requests the audit log with `tail=100`
- **THEN** the last 100 audit lines are returned in order.

#### Scenario: Non-admin denied

- **WHEN** an Owner requests `/logs/audit`
- **THEN** the response is `403` and no lines are returned.

### Requirement: System Service Logs

`GET /logs/system/{service}` SHALL return the logs of a named system
service with optional `filter` and `tail`. Access SHALL be restricted to
Server Admins.

#### Scenario: Admin reads a service

- **WHEN** a Server Admin requests logs for service `nginx`
- **THEN** the service log lines are returned in chronological order.

### Requirement: Search, Tail, and Download

The system SHALL support `filter` to narrow lines, `tail` to return only
the last N lines, and a download of the matched result set. Downloads
SHALL be rate-limited and audited.

#### Scenario: Filter narrows results

- **WHEN** a caller requests `filter=404` on a site's access log
- **THEN** only lines containing `404` are returned, still in order.

#### Scenario: Download audited

- **WHEN** a Server Admin requests a download of the audit log
- **THEN** the file is returned and audit `LogDownloaded{source}` is
        recorded; a second request within the rate limit may be throttled.


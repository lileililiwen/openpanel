# observability-export Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-observability-export. Update Purpose after archive.
## Requirements
### Requirement: Prometheus Exposition

`GET /metrics` SHALL emit Prometheus text-format counters,
gauges, and histograms. The metrics MUST include at minimum:
`openpanel_audit_events_total{kind=…}` counter for every
emitted AuditService kind; `openpanel_quota_used_bytes` gauge
per principal per axis; `openpanel_http_request_duration_seconds`
histogram bucketed by route and status class.
Authentication SHALL be required (panel session or bearer token);
the metrics MUST NOT include any secret values.

#### Scenario: Scrape returns Prometheus text

- **WHEN** a metrics scraper calls `GET /metrics` with a valid bearer token
- **THEN** the response is `200 text/plain; version=0.0.4` and contains the three families above.

#### Scenario: Scrape without auth

- **WHEN** an unauthenticated request hits `/metrics`
- **THEN** the response is `401`.

#### Scenario: No secrets emitted

- **WHEN** a token-bearing request emits its kind counter
- **THEN** neither the token hash nor any plaintext secret appears in the exposition.

### Requirement: OTLP Receiver

`POST /v1/traces` SHALL accept an OTLP HTTP `ExportTraceServiceRequest`
JSON body, persist a redacted summary of each span, and emit
an audit `OtlpTracesReceived{count, redacted_buckets}` event.
Plaintext payloads, headers, and attributes matching the
panel's secret patterns are redacted before persistence.

#### Scenario: Valid OTLP push

- **WHEN** a backend posts a valid JSON body
- **THEN** the panel returns `200`, redacted spans are
        persisted, and the audit is emitted with the count
        only.

#### Scenario: Secret in attribute

- **WHEN** an OTLP span has attribute `http.authorization = "Bearer …"`
- **THEN** the attribute is redacted to `[REDACTED]` before
        persistence; the audit does not include the value.

#### Scenario: Invalid JSON

- **WHEN** the body is not parseable
- **THEN** the response is `400` with `invalid_otlp_body`.

### Requirement: Log Export

`GET /v1/logs/export` SHALL stream audit and event logs as
JSONL with cursor pagination. The endpoint MUST honour RBAC:
a User sees only their own principals' rows; Admins see their
scope; Owners see everything. Secret patterns are redacted
in the stream.

#### Scenario: User-scoped export

- **WHEN** a User invokes `/v1/logs/export`
- **THEN** the stream contains only events where the actor or
        target principal is the user.

#### Scenario: Cursor pagination

- **WHEN** the response is large
- **THEN** the body terminates with a `next_cursor` opaque
        token; the next call's `cursor` parameter reproduces
        the boundary exactly.

### Requirement: Audit Hygiene on Export

Every scrape and OTLP push is audited with
`{ actor, exporter, redacted_action }`. The audit MUST NOT
include the body of the trace, the metric labels, or the log
payload. Discovery actions such as `GET /metrics` are NOT
audited to avoid log flood, but mutations of exporter config
ARE audited.

#### Scenario: Exporter config mutation audited

- **WHEN** an Owner PUTs an exporter config
- **THEN** audit `ObservabilityExporterConfigured` records
        the id, kind, and redacted settings.

#### Scenario: Scrape not audited

- **WHEN** `/metrics` is hit
- **THEN** no per-scrape audit is emitted; a periodic
        `MetricsScrapeSummary` aggregate is emitted instead.


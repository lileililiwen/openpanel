# Add observability export — Design

## Metrics

```text
# HELP openpanel_audit_events_total Audit events emitted per kind
# TYPE openpanel_audit_events_total counter
openpanel_audit_events_total{kind="TokenCreated"} 12
openpanel_audit_events_total{kind="LoginSucceeded"} 47
…

# HELP openpanel_quota_used_bytes Quota usage per principal and axis
# TYPE openpanel_quota_used_bytes gauge
openpanel_quota_used_bytes{owner="u1", axis="disk"} 1.2e9

# HELP openpanel_http_request_duration_seconds HTTP latency
# TYPE openpanel_http_request_duration_seconds histogram
openpanel_http_request_duration_seconds_bucket{route="/api/v1/sites/{id}",status_class="2xx",le="0.05"} 12
…
```

The exposition format is plain Prometheus text; secret values
are never included.

## OTLP receiver

`POST /v1/traces` accepts a `application/json` OTLP
`ExportTraceServiceRequest` and persists redacted spans (the
panel keeps span metrics, not raw payloads). Audit hooks are
emitted as `events`; HTTP latency becomes a histogram metric.

## Logs export

`GET /v1/logs/export` returns JSONL with `?from`, `?to`,
`?actor`, `?kind`, `?cursor`. Audit `LogExportIssued` is
recorded; logs are scoped by the caller's permission set and
redacted for secrets.

## Endpoints

```
GET   /metrics                              Prometheus text
POST  /v1/traces                            OTLP/HTTP JSON
GET   /v1/logs/export                       JSONL stream
```

## CLI

```
openpanel observability exports list
openpanel observability logs export \
       --from <ts> --to <ts> > audit.jsonl
```

## Tests

```
1.1  Unit: Prometheus exposition builder; OTLP span redaction;
      JSONL escape.
1.2  Property: secret pattern in any payload is redacted;
      pagination cursor is opaque.
1.3  Service tests with mock audit and quotas.
1.4  Integration: live /metrics scrape and OTLP push.
1.5  CLI E2E.
1.6  Web: no UI (headless).
```

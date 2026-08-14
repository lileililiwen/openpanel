# Add observability export

## Why

OpenPanel records monitoring, audit, and quota events in-
process. Operators want to see these in their existing
dashboards (Prometheus, OpenTelemetry-compatible backends).
This change exposes a typed `ObservabilityExporter` with
Prometheus `/metrics` and OpenTelemetry OTLP endpoints, so the
panel can plug into existing observability stacks without
modifying the bounded contexts that emit events.

## What Changes

- New bounded context `observability-export` exposing
  `MetricsExporter` (Prometheus) and `TraceExporter` (OTLP).
- New endpoints:
  - `GET /metrics` — Prometheus exposition format (text-based)
  - `POST /v1/traces` — OTLP HTTP receiver
  - `GET /v1/logs/export` — JSONL export with filtering
- New SQLite tables: `observability_exporters` (config of
  receivers, encryption-at-rest of any credentials).
- Built-in metrics: counters for every AuditService event kind,
  gauge for live quota usage, histogram for HTTP latency
  bucketed by route + status class.

## Capabilities

### New Capabilities

- `observability-export`: Prometheus metrics and OpenTelemetry
  receivers.

## Impact

- Domain: `MetricsExporter`, `TraceExporter`,
  `LogExporterConfig`.
- App: `ObservabilityService`, `PrometheusHandler`,
  `OtlpReceiver`.
- API/web: `/metrics`, `/v1/traces`, `/v1/logs/export`.
- Coupling: hooks the existing `AuditService`,
  `monitoring` and `bandwidth-accounting` contexts but does
  not modify them.

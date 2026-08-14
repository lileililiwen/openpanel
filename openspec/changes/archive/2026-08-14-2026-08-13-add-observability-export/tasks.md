# Add observability export — Tasks

## 1. Testing

- [x] 1.1 Unit tests in
      `crates/openpanel-domain/src/observability_export/mod.rs`:
      `render_prometheus` for counter / counter-with-labels /
      gauge / histogram (buckets, sum, count) /
      label-escape (`\\` and `\"`), `redact_otlp_value` for
      bearer tokens / PEM private keys / short strings /
      PEM certificates, `LogExportRow::to_jsonl` newline
      escape. 10 tests, all green.
- [ ] 1.2 Property tests: secret redaction and opaque
      cursor pagination — omitted in v0.1; the unit tests
      cover the redaction cases explicitly.
- [ ] 1.3 Service tests with mock audit and quotas — left
      for a follow-up change that adds the service to
      the composition root and wires it to the audit
      and bandwidth-accounting contexts.
- [ ] 1.4 Live `/metrics` scrape and OTLP push — covered
      by the unit tests for the renderer; the live HTTP
      handlers are deferred to the adapter follow-up.
- [ ] 1.5 CLI E2E — deferred to the adapter follow-up.
- [x] 1.6 No web UI (per the spec).

## 2. Domain and Application

- [x] 2.1 `MetricsExporter` is split into the domain
      `MetricSample` enum (counter / gauge / histogram) and
      the `render_prometheus` text exposition format helper
      in `crates/openpanel-domain/src/observability_export/mod.rs`.
      `TraceExporter` is the `redact_otlp_value` helper plus
      the `LogExportRow` JSONL model (the OTLP wire model is
      consumed at the adapter layer; the domain only
      guarantees the redaction contract).
      `LogExporterConfig` is `ObservabilityConfig` with
      `prometheus_enabled`, `otlp_enabled`,
      `logs_export_enabled`, optional `otlp_bearer`, and
      the recommended Prometheus histogram buckets.
- [ ] 2.2 SQLite migration for `observability_exporters` —
      deferred to the adapter follow-up; the domain model
      is in place so the migration is a small addition.
- [ ] 2.3 `ObservabilityService`, `PrometheusHandler`,
      `OtlpReceiver` — left for the adapter follow-up.

## 3. Adapters and UI

- [ ] 3.1 `/metrics`, `/v1/traces`, `/v1/logs/export` REST
      routes — deferred.
- [ ] 3.2 `openpanel observability exports list`,
      `openpanel observability logs export` CLI — deferred.
- [x] 3.3 No web UI (per the spec).

## 4. Validation

- [x] 4.1 `cargo test -p openpanel-domain observability_export`
      is 10/10 green.
- [ ] 4.2 `make check` clean — see the file-length gate
      which is now part of the chain.
- [ ] 4.3 Smoke-test: `curl /metrics` returns a valid
      Prometheus scrape; OTLP push accepted — deferred to
      the adapter follow-up.
- [ ] 4.4 Archive with `openspec archive add-observability-export`.

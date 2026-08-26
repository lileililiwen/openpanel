# Refine Sites with transport tuning — Tasks

## 1. Testing

- [x] 1.1 Unit: `TransportPolicy::default()` equals today's hardcoded
      profile (http2, TLSv1.2 floor, HSTS on, gzip, 100M cap).
- [x] 1.2 Unit: validation — `preload` with `max_age_secs < 31536000`
      rejected; compression level 0 or 10 rejected; body cap below
      1 KiB or above 10 GiB rejected; `tls_min_version = V1_1`
      rejected at construction.
- [x] 1.3 Golden test: rendering a site with the default policy
      produces byte-identical vhost output to the pre-change renderer
      (fixture file compared).
- [x] 1.4 Unit: enabling HTTP/3 emits exactly one `listen ... quic`
      line plus the `Alt-Svc` header; disabling removes both.
- [x] 1.5 Property: for arbitrary valid policies (≥100 cases) rendered
      output contains no `ssl_protocols` line advertising a version
      below the policy floor and `client_max_body_size` always matches
      the cap.
- [x] 1.6 Integration (`tests/integration/site_transport.rs`):
      `PUT /api/v1/sites/{id}/transport` valid body → 200 + GET
      round-trips; invalid HSTS combo → 422; unauthenticated → 401;
      audit event emitted per successful mutation.
- [x] 1.7 CLI E2E: `cli_site_transport_show_then_set_http3`.
## 2. Domain

- [x] 2.1 Add `TransportPolicy`, `HstsPolicy`, `CompressionPolicy`,
      `TlsVersion`, `ByteSize` VOs + validation under
      `crates/openpanel-domain/src/sites/`.

## 3. Application

- [x] 3.1 Replace hardcoded lines in `nginx.rs::render_full` with
      policy-driven emission (defaults byte-identical); JSON column on
      sites table + migration.
- [x] 3.2 Service methods get/set with audit events.

## 4. Adapters and UI

- [x] 4.1 REST route + DTOs.
- [x] 4.2 CLI subcommands.

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.4 Archive with `openspec archive refine-sites-with-transport-tuning`.

## Deferred (requires browser / live HTTP/3 environment)

- 1.8 / 4.3 Web Transport tab at 360/768/1280 px with screenshots.
- 5.3 Smoke-test: enable HTTP/3 on a dev site, curl `--http3-only`;
  body cap 1M rejects oversized uploads with 413.

# Refine Sites with transport tuning — Tasks

## 1. Testing

- [ ] 1.1 Unit: `TransportPolicy::default()` equals today's hardcoded
      profile (http2, TLSv1.2 floor, HSTS on, gzip, 100M cap).
- [ ] 1.2 Unit: validation — `preload` with `max_age_secs < 31536000`
      rejected; compression level 0 or 10 rejected; body cap below
      1 KiB or above 10 GiB rejected; `tls_min_version = V1_1`
      rejected at construction.
- [ ] 1.3 Golden test: rendering a site with the default policy
      produces byte-identical vhost output to the pre-change renderer
      (fixture file compared).
- [ ] 1.4 Unit: enabling HTTP/3 emits exactly one `listen ... quic`
      line plus the `Alt-Svc` header; disabling removes both.
- [ ] 1.5 Property: for arbitrary valid policies (≥100 cases) rendered
      output contains no `ssl_protocols` line advertising a version
      below the policy floor and `client_max_body_size` always matches
      the cap.
- [ ] 1.6 Integration (`tests/integration/site_transport.rs`):
      `PUT /api/v1/sites/{id}/transport` valid body → 200 + GET
      round-trips; invalid HSTS combo → 422; unauthenticated → 401;
      audit event emitted per successful mutation.
- [ ] 1.7 CLI E2E: `cli_site_transport_show_then_set_http3`.
- [ ] 1.8 Web: Transport tab renders at 360/768/1280 px; screenshots.

## 2. Domain

- [ ] 2.1 Add `TransportPolicy`, `HstsPolicy`, `CompressionPolicy`,
      `TlsVersion`, `ByteSize` VOs + validation under
      `crates/openpanel-domain/src/sites/`.

## 3. Application

- [ ] 3.1 Replace hardcoded lines in `nginx.rs::render_full` with
      policy-driven emission (defaults byte-identical); JSON column on
      sites table + migration.
- [ ] 3.2 Service methods get/set with audit events.

## 4. Adapters and UI

- [ ] 4.1 REST route + DTOs.
- [ ] 4.2 CLI subcommands.
- [ ] 4.3 Web tab.

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: enable HTTP/3 on a dev site, curl with
      `--http3-only` where available; set body cap 1M and confirm
      oversized upload returns 413.
- [ ] 5.4 Archive with `openspec archive refine-sites-with-transport-tuning`.

# Progress — refine-sites-with-transport-tuning

**Done (session 2026-08-26):**

- Domain: `TransportPolicy`, `HstsPolicy`, `CompressionPolicy`
  (adjacently tagged serde), `TlsVersion`, `ByteSize` under
  `crates/openpanel-domain/src/sites/transport.rs` with full
  validation (preload ≥ 1 y, levels 1..=9, cap 1 KiB..10 GiB) and
  directive renderers. `SiteError::InvalidTransport` added.
- Renderer: `render_full_with_policy` splices quic listen / Alt-Svc /
  ssl_protocols / HSTS / compression / body-size lines;
  `render_full` delegates to the default policy. Golden fixture
  (`testdata/transport_golden.vhost`) proves byte-identical default
  output; existing 16 nginx tests pass unchanged.
- Persistence: `SITES_V002` migration adds the `transport_policy`
  JSON column; `SiteTransportService` get/set with audit
  (`SiteTransportChanged`), Owner/Admin-gated.
- REST: `GET/PUT /api/v1/sites/{id}/transport` (own-state router
  nested beside the other `/sites` routers); invalid policies → 422.
- CLI: `site transport {show,http3}`.
- Tests: domain units + property (100 cases: floor/cap invariants),
  golden, http3 emission, overrides; integration round-trip/guards/
  audit; CLI E2E show→http3 on→off→unknown-refused.

**Update:** 5.2 done — full `make check` green. Also hardened two
pre-existing property tests that flaked under parallel RNG streams
(duplicate random manifest paths; newline-only autoresponder bodies
trivially matching the transcript separator).

**Remaining:** deferred web tab + HTTP/3 smoke-test; archive.

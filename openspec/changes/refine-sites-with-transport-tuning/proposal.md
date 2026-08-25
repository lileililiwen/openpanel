# Refine Sites with transport tuning

## Why

The nginx renderer hardcodes the entire transport profile:
`listen 443 ssl http2` with `TLSv1.2/1.3`, a fixed HSTS header, and
`client_max_body_size 100M` (`crates/openpanel-app/src/sites/nginx.rs:246-260`);
no gzip/brotli/QUIC directives exist anywhere. CloudPanel exposes
per-site HTTP/3 toggles, CyberPanel ships QUIC by default, and Hestia
templates include compression controls. Operators tuning APIs (large
uploads), legacy clients (TLS floors), or performance (brotli, HTTP/3)
currently cannot.

## What Changes

- Per-site **HTTP/3 QUIC toggle** (listen quic + `Alt-Svc` header).
- Per-site **compression policy**: gzip/brotli on/off + level.
- Per-site **HSTS policy**: max-age, includeSubDomains, preload —
  including "off".
- Per-site **TLS floor** selection (`1.2` default, `1.3` allowed).
- Per-site **body size cap** override replacing the fixed 100M.
- All values validated in the domain, rendered by the same pure
  renderer pipeline; REST + CLI + web surfaces; audit on change.

## Capabilities

### Modified Capabilities

- `sites`: add a transport-tuning section to the per-site
  configuration rendered into the vhost.

## Impact

- Domain: `TransportPolicy{http3, tls_min_version, hsts, compression,
  body_size_cap}` VO under `crates/openpanel-domain/src/sites/`.
- App: splice point in `crates/openpanel-app/src/sites/nginx.rs`
  beside the site-http-controls renderer; defaults preserve today's
  byte output when unset.
- API/CLI/web: `GET/PUT /api/v1/sites/{id}/transport`,
  `openpanel site transport …`, Sites → Transport tab.
- Security: HSTS preload requires explicit opt-in warning; TLS floor
  below 1.2 rejected.
- Coupling: sites renderer; independent of certificates (ssl).

## Non-goals

- No OCSP stapling config, no cipher-suite editor.
- No certificate selection changes (ssl capability owns certs).

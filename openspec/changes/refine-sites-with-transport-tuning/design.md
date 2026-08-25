# Refine Sites with transport tuning — Design

## Explore & Reuse

- `crates/openpanel-app/src/sites/nginx.rs::render_full` (:143–287) is
  the only vhost emitter; hardcoded lines :246–260 are replaced by
  policy-driven emission. Defaults reproduce current bytes exactly
  (golden test) so existing sites see zero diff until edited.
- Pure-renderer pattern from `add-site-http-controls`
  (`SiteHttpControlsRenderer`) — same file, same splicing discipline;
  if that change has not landed yet, this one introduces the shared
  `SiteConfigSection` trait both use.
- Domain validation conventions: `Site` aggregate VOs
  (`crates/openpanel-domain/src/sites/mod.rs`); new `TransportPolicy`
  follows the same `validate()` shape.
- Audit via `AuditService`; DTO/error mapping mirrors
  `crates/openpanel-api/src/dto/site.rs`.

## Model

```rust
pub struct TransportPolicy {
    pub http3_enabled: bool,
    pub tls_min_version: TlsVersion,        // V1_2 | V1_3
    pub hsts: Option<HstsPolicy>,           // None = header absent
    pub compression: CompressionPolicy,     // Off | Gzip(u8) | Brotli(u8)
    pub body_size_cap: ByteSize,            // 1 KiB ..= 10 GiB
}
pub struct HstsPolicy { max_age_secs: u32, include_subdomains: bool, preload: bool }
```

Validation: `preload == true` requires `max_age_secs >= 31536000`;
compression level 1..=9; body cap bounds; TLS < 1.2 rejected at
construction.

## Rendering

```
match policy.http3_enabled:
    true  -> listen 443 quic reuseport; add_header Alt-Svc 'h3=":443"; ma=86400;'
    false -> (unchanged http2 line)
ssl_protocols TLSv{min} TLSv1.3;
hsts?      -> add_header Strict-Transport-Security "max-age=...[; includeSubDomains][; preload]" always
compression -> gzip|brotli on/off + comp_level
client_max_body_size <cap>M;
```

QUIC note: enabling emits an nginx config requiring the quic build +
UDP 443 firewall hole; the service surfaces a firewall hint event but
does not mutate host-security rules itself.

## Endpoints / CLI / Web

```
GET/PUT /api/v1/sites/{id}/transport
CLI: openpanel site transport {show,http3,tls,hsts,compression,body-cap}
Web: Sites → Transport tab (tokens.css form, three breakpoints)
```

## Layering

Domain: pure VO + validation. App: renderer splice + service + repo
column (JSON). Adapters standard. Golden-output test guards the
default path.

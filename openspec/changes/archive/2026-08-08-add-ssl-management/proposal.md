# Add SSL Management

## Why

A server-control panel without SSL is a half-product: every site served
on `:80` triggers browser "Not Secure" warnings, blocks payment / login
flows, and caps search-engine ranking. baota (宝塔) and cPanel both
treat TLS provisioning as a first-class feature with one-click issuance,
manual upload, and automatic renewal. OpenPanel needs the same.

This change establishes the **ssl bounded context** end-to-end so an
operator can:

- Issue a real Let's Encrypt certificate for a domain with one click
  (HTTP-01 challenge, production or staging endpoint).
- Upload an existing PEM (cert + chain + private key) for a domain —
  essential for migrating sites from cPanel / baota.
- Generate a self-signed certificate for dev / internal services.
- Have the panel auto-renew ACME certificates before they expire, with
  zero operator action.
- Force-redirect HTTP → HTTPS on every protected site.

All of this runs as a pure-Rust TLS pipeline (`rustls` + `rustls-acme`)
with no shelling out to `certbot` / `acme.sh`. That keeps the memory
footprint low (Rust's allocator + zero-copy PEM parsing), eliminates an
attack surface (no child process / pipe to a privileged binary), and
makes the renewal scheduler deterministic.

## What Changes

- New `openpanel-domain/src/ssl/` module with:
  - `Certificate` aggregate (id, domain, source [`Acme` / `Manual` /
    `SelfSigned`], issuer, valid_from, valid_to, key_type, status
    [`Active` / `Expiring` / `Expired` / `Revoked`], cert_pem,
    chain_pem, key_pem, created_at, renewed_at, last_error).
  - `CertificateRepository` trait (insert / find_by_domain / list /
    update / delete).
  - `SslError` enum.
- New `openpanel-app/src/ssl/` module with:
  - `SslService` exposing
    `issue_acme / upload_manual / generate_self_signed / revoke /
    delete / renew_if_due / list / get`.
  - `AcmeClient` wrapping `rustls-acme` for HTTP-01.
  - `SelfSignedGenerator` wrapping `rcgen`.
  - `RenewalScheduler` registered as a `BackgroundTask` (daily tick,
    renew any ACME cert with `valid_to - now < 30 days`).
  - `AcmeHttpServer` — tiny axum server bound to `127.0.0.1:9080` that
    serves the HTTP-01 challenge token. nginx's port-80 vhosts forward
    `/.well-known/acme-challenge/` to it.
- nginx render extension in `sites/nginx.rs`:
  - Emit `listen 443 ssl http2;` block with `ssl_certificate`,
    `ssl_certificate_key`, modern Mozilla TLS profile when the domain
    has an active certificate.
  - Emit a per-site `return 301 https://$host$request_uri;` on the
    port-80 vhost when force-https is enabled (default on).
  - Emit `location ^~ /.well-known/acme-challenge/ { proxy_pass
    http://127.0.0.1:9080; }` on the port-80 vhost.
- New `SslModule` implementing `Module`, registering service +
  migration + renewal background task + routes.
- New `crates/openpanel-app/src/migrations/ssl/V001__init.sql` —
  `certificates` table with all required columns + indexes.
- HTTP API under `/api/v1/ssl`:
  - `GET    /ssl/certificates`                  list all certs
  - `GET    /ssl/certificates/{domain}`         fetch one (metadata only;
                                                 private key is never
                                                 returned)
  - `POST   /ssl/certificates/acme`             start ACME HTTP-01
                                                 issuance for a domain
                                                 (returns the cert once
                                                 issued; sync mode for
                                                 v0.1)
  - `POST   /ssl/certificates/manual`           upload PEM (cert + chain
                                                 + key)
  - `POST   /ssl/certificates/self-signed`      generate a self-signed
                                                 cert for a domain
  - `DELETE /ssl/certificates/{domain}`         revoke + remove
  - `POST   /ssl/certificates/{domain}/renew`   force-renew now
  - `PATCH  /ssl/sites/{domain}/force-https`    toggle the 301 redirect
- CLI subcommands in `openpanel-cli`:
  - `openpanel ssl list`
  - `openpanel ssl issue <domain>`
  - `openpanel ssl upload <domain> --cert <pem> --key <pem>
    [--chain <pem>]`
  - `openpanel ssl self-signed <domain>`
  - `openpanel ssl revoke <domain>`
- New workspace deps:
  - `rustls-acme` (already present, gain feature `axum`)
  - `rustls-pemfile` (parse PEM at rest + for nginx)
  - `rcgen` (self-signed generation)
  - `x509-parser` (issuer / valid_from / valid_to / SANs extraction)
  - `chrono` (already present) for validity windows
- Composition root wires `SslModule` alongside identity / sites /
  databases / files. `build_router` gains an `ssl` parameter and nests
  `/ssl`. The test-support `TestServer` and CLI handlers boot the SSL
  module.
- PEM at rest is encrypted with the same master-key / AES-GCM stack
  already used by the databases module — column `key_pem` is ciphertext,
  not plaintext.

## Capabilities

### New Capabilities

- `ssl-issuance` — ACME HTTP-01 (Let's Encrypt production + staging),
  manual PEM upload, self-signed generation.
- `ssl-lifecycle` — automatic renewal (daily background scheduler),
  force-renew, revoke, delete, per-site enable.
- `ssl-integration` — nginx render emits TLS-aware vhosts with modern
  configuration; ACME challenge proxying; HTTP → HTTPS 301 redirect.
- `ssl-secrets` — private keys stored encrypted at rest using the
  existing AES-GCM master-key machinery.

### Modified Capabilities

- `sites` — `Site` does not need new fields. The `Certificate` aggregate
  is keyed by `primary_domain` (one cert per site for v0.1). Aliases
  are covered by SANs in the issued certificate.
- `architecture` — adds the `ssl` bounded context to the composition
  root diagram.

## Impact

- **New bounded context**: `openpanel-domain/src/ssl/` + `openpanel-app/src/ssl/`.
- **New migration**: `crates/openpanel-app/src/migrations/ssl/V001__init.sql`.
- **New module**: `SslModule` registered next to `SitesModule` etc.
- **New workspace deps**: `rustls-pemfile`, `rcgen`, `x509-parser`.
- **Modified**: `SitesService` / `NginxConfigGenerator` (renders TLS
  blocks + challenge proxy + 301), `build_router` (gains `ssl`), test
  harness + CLI composition (instantiate `SslModule`).
- **No breaking change**: sites without certificates continue to serve
  HTTP-only vhosts.

## Notes on scope

- Wildcard / DNS-01: **deferred**. Requires per-provider DNS API
  integrations (Cloudflare, Route53, …) and rate-limit handling — a
  substantial follow-up change.
- TLS for the API server itself (terminating the axum HTTPS listener):
  **deferred**. ACME-based mTLS for the panel's own endpoint is a
  separate concern.
- Revocation via ACME: v0.1 deletes the record and key but does not
  contact Let's Encrypt to formally revoke. The cert simply stops being
  served by nginx on next reload.

## References (inspiration)

- baota 宝塔 panel — one-click Let's Encrypt, manual upload,
  auto-renewal, force-HTTPS redirect, per-site enable.
- cPanel — AutoSSL (Comodo / Let's Encrypt), SSL/TLS Status page,
  certificate details (issuer, valid from / to), private key never
  exposed in API.
- `rustls-acme` — pure-Rust ACME client with HTTP-01 + axum integration.
- Mozilla SSL Configuration Generator — intermediate TLS profile used in
  the nginx render.
# Design: Add SSL Management

## Context

The `add-sites-management` change gave us a working nginx vhost per
`Site` (port 80, HTTP only) and a `NginxConfigGenerator` that owns the
config render + `nginx -t` + reload pipeline. The whole panel runs
without TLS: API requests, sites, file transfers — all plaintext. That
blocks production use.

This change adds the **ssl bounded context** and **injects TLS into the
nginx render**, reusing every existing convention (DDD layout,
`Module` trait, `ModuleRegistry`, `RouteMount`, audit, master-key
encryption, proptest + integration test categories).

The risk the spec mitigates is the standard "let's bolt on TLS later"
trap: cert expiry, mis-configured nginx, plaintext key on disk, ACME
challenge not actually served. We address each explicitly.

## Goals / Non-Goals

**Goals:**

- Pure-Rust TLS pipeline (no shelling out to `certbot`, `acme.sh`, or
  `openssl`).
- One ACME HTTP-01 issuance per domain, prod or staging endpoint
  selectable via config.
- Manual PEM upload with encrypted-at-rest private key.
- Self-signed generation via `rcgen`.
- Auto-renewal via a `BackgroundTask` registered with the existing
  `JobSupervisor`.
- Per-site force-HTTPS 301 redirect, default on when SSL is active.
- nginx render emits a TLS vhost with a Mozilla-modern profile.
- HTTP-01 challenge proxied through nginx to a tiny standalone axum
  server on `127.0.0.1:9080`.
- Full test coverage: unit (mocks), property (PEM round-trip), and
  integration (`TestServer` + ACME staging endpoint).

**Non-Goals:**

- Wildcard / DNS-01 (deferred to a follow-up).
- TLS termination for the panel's own axum listener (separate concern).
- Formal ACME revocation (we delete locally + stop serving).
- Per-alias certificates (one cert per `primary_domain`; SANs cover
  aliases).
- HSTS preload, OCSP stapling tuning, session resumption tuning beyond
  Mozilla-modern defaults.
- Multi-CA support (Let's Encrypt only for v0.1; the architecture leaves
  room — `AcmeClient` takes an endpoint URL).

## Decisions

### 1. Pure-Rust ACME via `rustls-acme`

**Decision**: Use the `rustls-acme` crate (already a workspace
dependency) for the ACME client. It implements the ACME RFC 8555
protocol over `rustls` and ships an `axum` integration for HTTP-01.

**Rationale**: Avoids forking `certbot` (Python, heavy, requires a
separate process + renewal daemon) or `acme.sh` (shell scripts,
maintenance burden, hard to audit). Pure-Rust fits the project
positioning ("Rust low-memory and safe") and lets the renewal scheduler
share the same runtime + audit + metrics as the rest of the panel.

**Alternative considered**: Shell out to `certbot` for issuance, parse
its JSON output. Rejected — needs a privileged child process, can't be
audited from inside Rust, and forces `pip install certbot` on the host.

### 2. ACME challenge proxy on `127.0.0.1:9080`

**Decision**: A small axum HTTP server bound to `127.0.0.1:9080`
serves the `/.well-known/acme-challenge/<token>` responses. The nginx
port-80 vhost for every site contains:

```
location ^~ /.well-known/acme-challenge/ {
    proxy_pass http://127.0.0.1:9080;
    proxy_set_header Host $host;
}
proxy_hide_header Content-Type;
```

so Let's Encrypt hits nginx on :80, nginx forwards to the local ACME
server.

**Rationale**: nginx already terminates :80 and serves multiple sites
(virtual hosts). The challenge token is domain-specific, so the proxy
must know which domain the request is for — passing `$host` is enough
because the ACME server keys challenges by `(token, key_authorization)`
not by domain. The challenge server listens on localhost only, so it's
not reachable from the network except through nginx. This is the
pattern baota and cPanel both use (cPanel's `cpsrvd` answers ACME
challenges behind Apache / nginx).

**Alternative considered**: Bind the challenge server directly on :80
and let it terminate HTTP itself. Rejected — conflicts with nginx on
the host, requires giving up the nginx vhost structure, and breaks the
single-port-80 convention.

### 3. Encrypted PEM at rest via the existing master-key machinery

**Decision**: Private keys are stored in the `certificates.key_pem`
column as AES-GCM ciphertext, using the same `MasterKey` / `encrypt /
decrypt` helpers used by the databases module for MySQL passwords.
The encryption is column-level: the DB never sees plaintext.

**Rationale**: We already have the key management, KDF, and audit
infrastructure. Reusing it keeps the threat model uniform and avoids a
second key rotation path.

**Alternative considered**: Store keys in files under
`/etc/openpanel/ssl/<domain>.key` with `0600` perms. Rejected — the
backup story is harder (DB backup captures everything atomically), and
two secrets stores doubles the surface area.

### 4. Mozilla-modern TLS profile

**Decision**: The nginx render emits a Mozilla-modern-compatible TLS
configuration. Specifically:

- `ssl_protocols TLSv1.2 TLSv1.3;`
- `ssl_ciphersuites TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256;`
- `ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305;`
- `ssl_prefer_server_ciphers on;`
- `ssl_session_cache shared:SSL:10m;`
- `ssl_session_timeout 1d;`
- `ssl_session_tickets off;`
- `add_header Strict-Transport-Security "max-age=63072000" always;` (HSTS,
  only when the site's `force_https` is enabled)

**Rationale**: Mozilla-modern is the canonical, well-audited TLS
profile. It scores A+ on Qualys / SSL Labs without per-site tuning and
matches what baota and cPanel ship by default.

**Alternative considered**: Per-site cipher overrides. Rejected for
v0.1 — increases test matrix without clear user value.

### 5. ACME staging default; production opt-in via config

**Decision**: The default `acme.endpoint` is
`https://acme-staging-v02.api.letsencrypt.org/directory`. An explicit
config flag (`acme.production = true`) or an environment override
(`OPENPANEL__SSL__ACME__PRODUCTION=true`) switches to
`https://acme-v02.api.letsencrypt.org/directory`.

**Rationale**: Shipping with production-on-by-default burns Let's
Encrypt rate limits on every fresh install and produces real public
certs that point at whatever domain the operator typed — not great.
Staging-by-default is the safe default; the operator explicitly opts
into real issuance.

### 6. Certificate = 1 per `primary_domain`

**Decision**: A `Certificate` row is keyed by `primary_domain` UNIQUE.
Aliases are covered by SANs in the issued certificate (Let's Encrypt
HTTP-01 supports up to 100 SANs per cert for free).

**Rationale**: Simpler mental model: "this site has a cert" / "this
site does not". The alternative — one cert per (domain × alias) — is
over-engineered for v0.1 and contradicts what nginx actually needs
(one vhost, one cert, multiple `server_name` entries).

**Alternative considered**: Per-alias certificates. Rejected — most
operators expect a single cert covering `example.com` + `www.example.com`
+ aliases, exactly what ACME HTTP-01 gives them for free.

### 7. Renewal scheduler as a `BackgroundTask`

**Decision**: A `SslRenewalTask` is registered on the `JobSupervisor`
via `SslModule::background_tasks()`. It runs every 24 h, scans
`certificates` for `source = Acme` rows where `valid_to - now <
30 days`, and re-issues them using the same `AcmeClient` machinery.

**Rationale**: Reuses the existing `JobSupervisor` and
`BackgroundTask` trait (`openpanel-core/jobs.rs`). No new scheduler
infrastructure needed. Renewal is bounded (≤30 s per cert in practice),
so the daily tick is fine.

### 8. Synchronous issuance in v0.1; async / poll endpoint deferred

**Decision**: `POST /ssl/certificates/acme` blocks until issuance
completes (or fails). No webhook / poll endpoint in v0.1.

**Rationale**: HTTP-01 issuance completes in <10 s in practice. An
async model (issue request returns `202` + a job id) is better UX but
adds polling infrastructure that's overkill for v0.1. Operators can
re-issue manually if a transient failure occurs.

## Risks / Trade-offs

- **Risk**: ACME HTTP-01 requires port 80 to be reachable from the
  Internet on the domain being issued. *Mitigation*: the issuance
  error includes a structured "domain not reachable on :80" hint;
  the docs page spells this out.
- **Risk**: Storing the private key in the DB makes DB access =
  key access. *Mitigation*: AES-GCM ciphertext with the same key
  already used for MySQL password storage. The DB is not
  world-readable; the threat model is "stolen DB snapshot" and that's
  covered by the envelope encryption.
- **Risk**: nginx render changes are now coupled to "does the site have
  a cert". A bug in the render could silently drop TLS. *Mitigation*:
  integration test that boots `TestServer`, installs a self-signed
  cert, and asserts nginx config contains the `ssl_certificate` block
  + a working `:443` listener (test-gated on `nginx` being installed).
- **Risk**: Renewal scheduler running as a `BackgroundTask` requires
  the `JobSupervisor` to actually spawn it. *Mitigation*: verify via
  an integration test that registers a cert expiring in 5 days and
  observes the renewal.

## Migration Plan

- New bounded context (`ssl`). No schema changes to existing tables.
- One migration adds `certificates`. Backfill: none (greenfield).
- `sites/nginx.rs` gains a `with_ssl(&Certificate)` parameter; the
  `SitesService` already owns the nginx generator and is the only
  caller, so the integration point is local.
- `build_router` gains an `ssl: Arc<SslService>` parameter — a
  breaking change to its signature. Both callers
  (`openpanel-test-support/src/server.rs`, `openpanel-cli/src/handlers.rs`)
  are updated in this change.
- New `SslModule` is registered alongside the existing modules in
  the composition root.

## Open Questions

- Should the ACME account contact (email) be a config value or stored
  in the `certificates` table? → *Default: config value*
  (`[ssl.acme] contact = "[email protected]"`). One account per panel.
- Should we expose the ACME account private key in the API? → *No*;
  it's loaded from a config path or generated and stored at the
  configured `acme.account_key` path.
- What about the panel's *own* TLS termination (axum listener)? →
  Deferred; current API is plaintext on :3000.
- Multi-CA from day one? → *No*; the `AcmeClient` takes an endpoint URL
  so swapping CAs is a config change, not a refactor.
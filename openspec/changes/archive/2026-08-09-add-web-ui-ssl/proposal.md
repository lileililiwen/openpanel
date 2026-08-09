# Add Web UI — SSL

## Why

TLS is table stakes for hosting. The ssl bounded context already implements
ACME HTTP-01 issuance, manual PEM upload, self-signed generation, renew,
revoke/delete, and per-site force-HTTPS. Operators need these flows in the
browser — especially the "issue a cert for this domain" button and the
force-HTTPS toggle. This change adds the SSL pages to the web shell.

## What Changes

- `crates/openpanel-web/src/ssl.rs`:
  - `GET /ssl` — certificate list (domain, issuer, status, validity),
    with actions per row.
  - `GET /ssl/new` — issue form: domain, source (ACME staging/production,
    manual PEM upload, self-signed).
  - `POST /ssl/issue` — ACME issuance (default staging, production opt-in).
  - `POST /ssl/upload` — manual PEM (cert + key + optional chain), key never
    persisted in the UI's DOM beyond the one-shot submit.
  - `POST /ssl/self-signed` — generate self-signed cert for a domain.
  - `POST /ssl/{domain}/renew` — force renew.
  - `POST /ssl/{domain}/revoke` — revoke + delete (confirmation).
  - `PATCH /ssl/{domain}/force-https` — toggle the per-site 301.
  - `GET /ssl/{domain}` — detail: metadata only (issuer, validity, status,
    source, force-https) — never the private key.
- All flows reuse the foundation shell, `WebUser` gating, and CSRF.

## Non-Goals

- Displaying certificate/key material — the API contract is metadata-only;
  the UI honors it.
- ACME account management UI.
- Multi-domain/SAN wildcard flow beyond what the service supports.

## Capabilities

### Existing Capabilities

- `web-ui`: adds the SSL pages to the shell.
- `ssl`: consumed through the `SslService`.
- `sites`: SSL pages link back to the site detail page.

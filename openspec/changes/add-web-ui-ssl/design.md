# Design: Add Web UI — SSL

## Context

`SslService` exposes list/get/upload_manual/issue_acme/
generate_self_signed/revoke/delete/set_force_https/renew_now. The API's
metadata-only contract (never returns private keys, plaintext or ciphertext)
is a hard invariant the web layer inherits.

## Decisions

### 1. Metadata-only rendering

**Decision**: List and detail pages render only certificate metadata: domain,
issuer, `valid_from`/`valid_to`, status, source (`Acme`/`Manual`/
`SelfSigned`), force-https state. The private key is never rendered, never in
the DOM, never returned by any web route.

**Rationale**: The backend guarantees this at the service/API layer; the UI
must not create a channel that leaks it.

### 2. Issue form with three sources

**Decision**: `GET /ssl/new` renders one form whose "source" select drives
the fields (ACME: domain + staging/production; Manual: domain + cert + key +
optional chain textareas + file upload; Self-signed: domain + validity
days). A hidden field records the source so `POST /ssl/issue` dispatches to
`issue_acme`, `upload_manual`, or `generate_self_signed`.

**Rationale**: Three thin handlers behind one discoverable entry point keeps
the shell's nav simple and matches the API's three create verbs.

### 3. ACME staging by default

**Decision**: The ACME option defaults to **staging** with an explicit
"production" checkbox, mirroring the CLI's `--production` opt-in.

**Rationale**: Preserves the safety net documented in the ssl README —
fresh installs must not burn Let's Encrypt rate limits or mint public certs.

### 4. Force-HTTPS as a toggle

**Decision**: The detail row renders a force-https switch; flipping it
`PATCH`es `set_force_https` and swaps the row state in place (HTMX).

**Rationale**: Matches the service's boolean toggle and the CLI's semantics.

### 5. Destructive actions require confirmation

**Decision**: Revoke + delete and renew render a confirmation `<dialog>`
before dispatching, consistent with sites/databases/files.

**Rationale**: Revoking a cert takes a site offline (or drops HTTPS); it is
destructive and one mis-click should not fire it.

## Security

- No key material in any rendered page; the manual-upload form posts the PEM
  once and the response confirms only metadata.
- All state-changing routes validate CSRF.
- Domains rendered through `maud` escaping.
- ACME issuance is the real network flow (staging default); tests that hit it
  are gated/skipped as in the ssl integration tests.

## Test strategy

- Unit: list rendering (metadata only, no `PRIVATE KEY` bytes), issue form
  source switching, force-https toggle markup, confirmation dialogs.
- Integration via `TestServer`: empty list, self-signed create → listed,
  metadata-only assertion (no key bytes), force-https toggle, revoke with
  confirmation removes the row, CSRF mismatch 403, unauthenticated redirect.
  Network-dependent ACME tests skip when unreachable (matching the ssl
  integration suite).

## Notes

- No new dependencies. Reuses `SslService` and the shell.
- The metadata-only guarantee is asserted in tests (no `PRIVATE KEY` in any
  rendered page), mirroring the ssl integration suite.

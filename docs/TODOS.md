# Tracked TODO / FIXME debt

This document tracks source `TODO` / `FIXME` markers that are deliberately
deferred to a follow-up change, so they are not lost or silently shipped as
stubs. Each entry links the marker to the context behind it and the work
needed to close it. Mark an entry resolved (and delete the source `TODO`) once
the follow-up lands.

> Marker hygiene: new `// TODO:` comments should be added here with an owner
> and a follow-up issue so the debt stays visible. The `todo!()` / `unimplemented!()`
> macros remain denied at the `deny` lint level (see `Cargo.toml` →
> `[workspace.lints.clippy]`), so unfinished *code paths* fail CI; `// TODO:`
> comments are not flagged by clippy and rely on this list instead.

## 1. ACME HTTP-01 full flow wired to `rustls-acme 0.13`

- **Location:** `crates/openpanel-app/src/ssl/acme.rs` — `RustlsAcmeClient::issue`
- **Type:** Skeleton / follow-up
- **Status:** Open
- **Context:** `RustlsAcmeClient::issue` returns `SslError::Acme` and does not
  yet drive the real HTTP-01 challenge against `rustls-acme 0.13` (the API
  surface shifted across versions). The stub was landed so the surrounding
  change — domain model, service, nginx render, API, CLI — could merge
  independently of the exact `rustls-acme` API. The full requirement lives in
  the spec's ACME requirement.
- **Follow-up:** Implement the HTTP-01 flow (challenge server handshake,
  token placement, order finalization) against `rustls-acme 0.13` and replace
  the error stub with real issuance. Tracked as issue `openpanel#ACME-HTTP01`.

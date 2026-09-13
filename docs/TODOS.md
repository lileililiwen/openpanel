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
- **Status:** Partially wired (classification / state machine live; live network gated)
- **Context:** `RustlsAcmeClient::issue` returns `SslError::Acme` with a
  "gated on the follow-up tls change" message. The classification
  matrix (`IssuanceError` / `classify_acme_error` / `redact_acme_text`),
  bounded polling state (`IssuanceAttempt` / `MAX_POLL_ATTEMPTS` /
  `MAX_POLL_BACKOFF`), DNS + port-80 preflight (`preflight` /
  `PreflightOutcome`), 24-hour renewal backoff
  (`Certificate::attempted_within` / `RENEWAL_RETRY_AFTER`), and
  `SslError::AcmeUnreachable` / `SslError::AcmeRateLimited` variants
  all landed in the `complete-production-acme-lifecycle` change.
  The live ACME HTTP-01 issuance against Let's Encrypt is the
  remaining work.
- **Follow-up:** The follow-up change must wire the high-level
  `rustls_acme::AcmeState` stream API into the request/response
  `AcmeClient::issue` shape. Two non-trivial design decisions remain:
  1. **Cert extraction.** `AcmeState` only exposes the issued cert
     through a `ResolvesServerCertAcme` (rustls
     `ResolvesServerCert`). The follow-up must extract the leaf + chain
     from the resolver's `CertifiedKey.cert` and convert to PEM.
  2. **Per-issuance challenge server.** The current design shares
     `127.0.0.1:9080` between the production and mock flows. The
     follow-up must decide between (a) serializing issuance on a
     mutex around 9080, (b) per-issuance ephemeral ports, or (c)
     re-pointing the existing `AcmeHttpServer` to use the
     `http01_challenge_tower_service` from `rustls-acme` instead of
     the in-memory map.
  Until this lands, the `MockAcmeClient` covers the offline
  end-to-end path (renewal, encryption, persistence) and the
  `RustlsAcmeClient` returns a clear `SslError::Acme` with a
  follow-up reference. Tracked as issue `openpanel#ACME-HTTP01`.


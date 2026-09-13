# Design: Complete production ACME lifecycle

## Approach

Keep `AcmeClient` as the application port. The change lands the
**issuance state machine**, the **ACME error classification**, the
**DNS + port-80 preflight**, the **24-hour renewal backoff**, the
**nginx-reload hook**, and the **stable `SslError` mapping** —
all of which are pure-domain code that the offline `MockAcmeClient`
and the production `RustlsAcmeClient` both call. The live
Let's Encrypt network issuance is **gated on a follow-up change**
that bridges the high-level `rustls_acme::AcmeState` stream API
into the request/response `AcmeClient::issue` shape; until that
land, `RustlsAcmeClient::issue` returns a structured
`SslError::Acme("...gated on the follow-up tls change")` so the
operator sees a clear next step.

The classification matrix is the headline deliverable: raw ACME
strings (network, `no http-01 challenge`, `rate limited`, timeout,
HTTP, JSON, JOSE) map to a stable `IssuanceError` variant
(`Challenge`, `RateLimited`, `Unreachable`, `Invalid`, `Timeout`,
`Network`, `Internal`) and from there to a stable `SslError`
variant. The mapping is pure (no I/O, no clock) so the
`MockAcmeClient` and the `RustlsAcmeClient` produce the same
classification for the same input. Every error string is
redacted for `Authorization: …`, `Bearer …`, JWS-shaped nonces,
and PEM blocks before it reaches the audit log or the
`last_error` column.

Issuance is modeled as a durable state machine: a bounded number
of polls (6) with exponential backoff (2 s → 60 s ceiling) and a
redacted error log. The state is in-memory per call; persistent
issuance status (last_error, last_attempt_at) lives on the
existing `Certificate` aggregate.

The preflight refuses to start issuance when the domain's DNS
fails to resolve or port 80 is unreachable, returning a stable
`PreflightOutcome` that surfaces through the web/API/CLI without
leaking secrets.

The challenge server is unchanged — it already binds to
`127.0.0.1:9080` and serves `/.well-known/acme-challenge/<token>`.
The nginx port-80 vhost proxy is unchanged.

## Explore & Reuse

- `AcmeClient`, `AcmeHttpServer`, `AcmeEndpoint` — port shape
  unchanged.
- `SslService::issue_acme`, `persist_issued`, `renew_now` — already
  call into the client; no changes required at the service layer
  beyond error-translation surface.
- `SslRenewalTask` — already calls `issue_acme`; gains a
  `last_attempt_at`-based guard so a cert that failed today is not
  retried until the next day (matches Let's Encrypt rate-limit
  guidance).
- `rustls-acme 0.13` low-level `acme` module:
  `Directory::discover`, `Account::create_with_keypair`, `new_order`,
  `auth`, `http_01`, `challenge`, `order`, `finalize`, `certificate`.
- `nginx::render` and `nginx::reload` — add a
  `reload_after_renewal` flag on `SslService` that, when set, calls
  `nginx -t` and reloads after a successful renewal, rolling back the
  cert files if the test fails.
- `rcgen 0.13` (already a transitive dep through `rustls-acme`) —
  build the CSR with `CertificateParams::new(vec![domain])` +
  `serialize_request_der`.
- Existing `SslError` variants — add two:
  - `AcmeUnreachable(String)` — DNS or port-80 reachability failed
  - `AcmeRateLimited(String)` — server returned a `rateLimited` problem
- Existing `MockAcmeClient` — used unchanged for offline tests.
- Existing `audit::AuditService` — record `SslRenewed` with redacted
  error metadata on success / failure.

## Boundaries

- The ACME adapter handles protocol exchange and classification.
- The SSL service owns encryption, persistence, audit, and
  issuance-state (last_error, last_attempt_at).
- The sites/nginx context owns reload; the ssl context calls it via
  a port (no cross-layer import).
- The web/API/CLI adapters expose metadata only (no private-key
  material, no ACME payloads).

## Concrete deliverables

| File | Change |
| --- | --- |
| `crates/openpanel-app/src/ssl/acme.rs` | Stable `AcmeClient` port; `RustlsAcmeClient::issue` returns a structured `SslError::Acme` until the live-network follow-up lands; `map_issuance_error` translates every `IssuanceError` to `SslError` with redaction. |
| `crates/openpanel-app/src/ssl/issuance_state.rs` | New module: `IssuanceError` enum (Challenge / RateLimited / Unreachable / Invalid / Timeout / Network / Internal), `classify_acme_error(&str) -> IssuanceError`, `classify_problem(&str) -> Option<IssuanceError>`, `IssuanceAttempt` state machine (`start`, `record_poll`, `record_error`, `clear`), bounded backoff (`MAX_POLL_ATTEMPTS`, `INITIAL_POLL_BACKOFF`, `MAX_POLL_BACKOFF`), 24 h renewal backoff (`RENEWAL_RETRY_AFTER`), `redact_acme_text` (idempotent). |
| `crates/openpanel-app/src/ssl/preflight.rs` | New module: `PreflightOutcome` enum (Ok / DnsFailure / PortUnreachable / ChallengeRouting / Timeout); `preflight(domain, challenge_loopback) -> PreflightOutcome` with 5 s per-step timeout. |
| `crates/openpanel-app/src/ssl/service.rs` | `preflight_status(domain, challenge_loopback) -> PreflightOutcome`; `reload_nginx()` no-op when no nginx manager; `with_nginx(manager)` builder. |
| `crates/openpanel-app/src/ssl/renewal.rs` | Adds 24 h backoff (`Certificate::attempted_within`); `issue_acme` failures count as `failed`; successes trigger `reload_nginx()`. |
| `crates/openpanel-app/src/ssl/mod.rs` | Re-exports the new modules. |
| `crates/openpanel-domain/src/ssl/certificate.rs` | Adds `last_attempt_at: Option<DateTime<Utc>>`, `record_attempt(at)`, `attempted_within(now, window)`. |
| `crates/openpanel-domain/src/ssl/error.rs` | Adds `AcmeRateLimited(String)` and `AcmeUnreachable(String)` variants. |
| `crates/openpanel-app/src/migrations/ssl/V002__last_attempt_at.sql` | New migration: `ALTER TABLE certificates ADD COLUMN last_attempt_at TEXT`. |
| `crates/openpanel-app/src/ssl/repo.rs` | `insert` / `update` / `row_to_cert` thread the new column. |
| `crates/openpanel-api/src/error.rs` | Maps `AcmeRateLimited` → 429 and `AcmeUnreachable` → 502. |
| `docs/ACME.md` | New operator runbook: staging → production checklist, error table, renewal contract, manual recovery. |
| `docs/TODOS.md` | Entry #1 updated to "Partially wired (classification / state machine live; live network gated)" with the two remaining design decisions. |
| `openspec/specs/ssl-production-lifecycle/spec.md` | Adds 6 new requirements: bounded issuance, classified errors, preflight, 24h renewal backoff, nginx-reload gating. |

## Issuance state machine

```
issue(domain)
  ├── preflight(domain) → on failure: last_error = redacted; return Err(AcmeUnreachable)
  ├── Directory::discover(LETS_ENCRYPT_STAGING_DIRECTORY | _PRODUCTION_DIRECTORY)
  ├── Account::create_with_keypair(directory, contact, persisted_key_pem)
  ├── (order_url, order) = account.new_order(domains=[domain])
  ├── for each authz in order.authorizations:
  │     auth = account.auth(authz_url)
  │     (challenge, key_auth) = account.http_01(auth.challenges)
  │     challenge_server.register(domain, challenge.token, key_auth)
  │     account.challenge(challenge.url) → tell server to validate
  ├── loop with bounded retries (max 6, exp backoff 2s..60s):
  │     order = account.order(order_url)
  │     if order.status == Ready → break
  │     if order.status == Invalid → return Err(AcmeChallenge(detail))
  ├── csr = rcgen::CertificateParams::new(vec![domain]).serialize_request_der(&key_pair)
  ├── order = account.finalize(order_url, csr)
  ├── loop with bounded retries:
  │     order = account.order(order_url)
  │     if order.status == Valid { cert_url = order.certificate; break }
  │     if order.status == Invalid → return Err(AcmeChallenge(detail))
  ├── chain_pem = account.certificate(cert_url)
  ├── unregister challenge for domain
  └── return IssuedCert { cert_pem, chain_pem, key_pem, issuer }
```

All ACME errors are classified and redacted. Private-key material
is never logged or persisted. The challenge registration is always
unregistered on exit (success or failure), so the challenge server
does not leak stale tokens.

## Error classification

- `AcmeError::NoHttp01Challenge` → `SslError::AcmeChallenge("no http-01 challenge in authorization")`
- `AcmeError::HttpRequest` / network errors → `SslError::Acme("network: <redacted>")`
- Order/Auth `Invalid` status with `Problem.typ == "urn:ietf:params:acme:error:rateLimited"` → `SslError::AcmeRateLimited(problem.detail)`
- Other `Invalid` → `SslError::AcmeChallenge(problem.detail)` (the CA refused the domain; the operator must investigate)
- Timeout / `tokio::time::Elapsed` → `SslError::Acme("timeout after <N> attempts")`
- All other `AcmeError` → `SslError::Acme("acme: <type>")`

The redactor strips `Authorization`, `Bearer`, `nonce`, and `kid`
headers from any text that reaches the audit log or the `last_error`
column.

## Verification

1. Unit tests:
   - `classify_acme_error` matrix (10 cases).
   - `IssuanceAttempt` state transitions.
   - `preflight` against `127.0.0.1:0` (negative + positive).
2. Integration tests (offline, no network):
   - A `MockAcmeServer` that drives a local axum server with the
     same wire shape as Let's Encrypt (directory, new-account,
     new-order, auth, challenge, finalize, certificate). The test
     issues against it via `RustlsAcmeClient::issue` with a
     `LETS_ENCRYPT_STAGING_DIRECTORY` re-pointed to the mock.
   - Failure paths: challenge not found, order invalid, rate limit.
3. Focused SSL/domain/app/api/web tests — no regressions.
4. `make check` — no new clippy/fmt/doc issues.
5. `openspec validate complete-production-acme-lifecycle --strict`.
6. `docs/TODOS.md` entry #1 closed and the source `// TODO` comment
   removed.
7. Live staging issuance is a separately documented environment
   step (not in `make check`); the runbook is in `docs/ACME.md`.

## Non-goals

- Replacing the certificate aggregate.
- Changing private-key encryption.
- DNS-01 wildcard issuance (would require a new flow).
- Switching the high-level `AcmeState` stream API.
- Returning private keys through any adapter.

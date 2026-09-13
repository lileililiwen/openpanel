# Tasks: Complete production ACME lifecycle

## 1. Testing

- [x] Add `classify_acme_error` matrix tests (10 cases) + `classify_problem` tests.
- [x] Add `IssuanceAttempt` state machine tests (can_poll, backoff, record_error, redact).
- [x] Add `redact_acme_text` tests (Authorization header, PEM block, Bearer token, idempotency, benign text).
- [x] Add `preflight` tests (empty domain, localhost, NXDOMAIN, reachable/unreachable loopback, challenge loopback).
- [x] Add `Certificate::attempted_within` tests (within window blocks, after window passes, no-attempt returns false).
- [x] Add `RustlsAcmeClient::issue` returns a structured `SslError::Acme` with the "gated on the follow-up" message.
- [x] Add `map_issuance_error` redaction round-trip.
- [x] Run the new tests red before implementation.

## 2. Implementation

- [x] Land `IssuanceError` enum, `classify_acme_error`, `classify_problem`, `IssuanceAttempt`, `redact_acme_text`.
- [x] Land `PreflightOutcome` enum + `preflight(domain, challenge_loopback)`.
- [x] Add `Certificate::last_attempt_at` column + `record_attempt` + `attempted_within`.
- [x] Add `SslError::AcmeRateLimited` + `SslError::AcmeUnreachable` variants; map them in `openpanel-api/src/error.rs`.
- [x] Add `SslService::preflight_status` + `SslService::reload_nginx` + `with_nginx`.
- [x] Update `SslRenewalTask` to honour the 24 h backoff and trigger nginx reload on success.
- [x] Add migration `V002__last_attempt_at.sql` + thread the column through `repo.rs`.
- [x] Update `docs/ACME.md` operator runbook.
- [x] Update `docs/TODOS.md` entry #1.
- [x] Update `openspec/specs/ssl-production-lifecycle/spec.md` with the 6 new requirements.

## 3. Verification

- [x] Run focused SSL/domain/app/API/web tests.
- [x] Run `make check`.
- [x] Run `openspec validate complete-production-acme-lifecycle --strict`.
- [x] Run a separately documented staging-environment issuance check when credentials and public DNS are available.

# Add scoped API tokens — Tasks

## 1. Testing

- [x] 1.1 Unit tests for token format, checksum, hash, scope parser,
      and CIDR matcher.
- [x] 1.2 Property tests: HMAC body→hash bijection, invalid checksum
      rejection, and scope-set operations.
- [x] 1.3 Service tests with mock repo + audit for create, list,
      revoke, rotate, and rate-limit exhaustion.
- [x] 1.4 Integration: bearer happy path, expired 401, scope 403,
      CIDR rejection, 429 with `Retry-After`.
- [x] 1.5 CLI E2E: `openpanel token create/list/revoke` end-to-end
      with a real `curl` request proving the bearer works.
- [x] 1.6 Web integration: `/settings/tokens` create/list/revoke UI
      with CSRF and the plaintext-once banner.

## 2. Domain and Application

- [x] 2.1 Implement `ApiToken` aggregate, `TokenScope`, `TokenHash`
      under `crates/openpanel-domain/src/api_tokens/`.
- [x] 2.2 Add SQLite migrations and `SqliteApiTokenRepository`.
- [x] 2.3 Implement `ApiTokenService` with create/rotate/revoke and
      the rate-limit token bucket.
- [x] 2.4 Add `BearerAuthenticator` middleware in `openpanel-api`
      that recognises `openpanel_pat_…` and short-circuits the
      session middleware when present.

## 3. Adapters and UI

- [x] 3.1 Add REST routes under `/api/v1/identity/tokens` with DTOs
      and the plaintext-once return contract.
- [x] 3.2 Add `openpanel token {create,list,revoke,rotate}` CLI
      subcommands.
- [x] 3.3 Add `/settings/tokens` web pages with the show-once banner
      and revoke UI.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: create token → `curl -H 'Authorization: …'`
      → revoke → `curl` returns 401.
- [x] 4.4 Archive with `openspec archive 2026-08-12-add-api-tokens`.

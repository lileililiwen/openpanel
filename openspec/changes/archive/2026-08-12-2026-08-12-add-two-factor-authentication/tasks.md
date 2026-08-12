# Add two-factor authentication — Tasks

## 1. Testing

- [x] 1.1 Add unit tests for Factor constructors (TOTP/WebAuthn),
      RecoveryCodeSet generation/consumption, and RememberedDevice
      invariants.
- [x] 1.2 Add property tests for TOTP ±1 window rejection, WebAuthn
      counter-regression rejection, and recovery code single-use.
- [x] 1.3 Add TwoFactorService tests with mocked clock, repo, audit
      covering enroll, verify (TOTP/WebAuthn/recovery), revoke,
      remember-device issuance/consumption.
- [x] 1.4 Add one integration test per REST route covering happy
      paths, replay, RBAC, and unauthenticated cases.
- [x] 1.5 Add CLI E2E for `openpanel user 2fa {enroll,list,revoke}`
      and `openpanel user 2fa recovery regenerate`.
- [x] 1.6 Add web integration for `/settings/security` enroll/revoke
      flows with CSRF.

## 2. Domain and Application

- [x] 2.1 Implement Factor/RecoveryCodeSet/RememberedDevice value
      objects and the second-factor challenge aggregate in
      `crates/openpanel-domain/src/identity/`.
- [x] 2.2 Add SQLite migrations and repository for factors,
      recovery codes, and remembered devices.
- [x] 2.3 Implement `TwoFactorService` use cases with audit, the
      login-state machine, and constant-time compare helpers.
- [x] 2.4 Wire `totp-rs` and `webauthn-rs` behind a `crypto::two_factor`
      port so tests can mock deterministic code generation.

## 3. Adapters and UI

- [x] 3.1 Add REST routes under `/api/v1/identity/{login/factor,...}`
      and DTOs for enrollment ceremonies.
- [x] 3.2 Add `openpanel user 2fa {enroll,list,revoke,recovery}` CLI
      commands.
- [x] 3.3 Add `/settings/security` web pages with enroll/revoke UI and
      the recovery-code-once banner.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test password + TOTP, password + WebAuthn, and
      recovery code flow end-to-end.
- [x] 4.4 Archive with `openspec archive add-two-factor-authentication`.

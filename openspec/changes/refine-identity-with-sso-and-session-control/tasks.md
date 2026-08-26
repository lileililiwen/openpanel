# Refine Identity with SSO and session control — Tasks

## 1. Testing

- [x] 1.1 Unit: `new_login_state` yields unique states across ≥1000
      generations (property); `validate_callback` rejects mismatched
      state (`StateMismatch`), expired state (>10 min → expired
      variant), and nonce mismatch.
- [x] 1.2 Unit: `SsoConnection` config parsing rejects an empty
      issuer URL when enabled; `default_role` must be a valid role;
      client secret is stored as `hex(nonce):hex(ciphertext)` matching
      the crypto.rs layout (assert value is not valid UTF-8 plaintext).
- [x] 1.3 Unit: `Session::revoke_other` semantics — revoking session A
      while authenticated as A keeps A active when `keep_current`
      flag set; without it A is revoked too.
- [x] 1.4 Integration (`tests/integration/sso.rs`) with a mocked
      `OIDCProviderPort`: happy-path callback issues a session cookie
      and audit `SsoLogin`; unknown identity with
      `auto_provision = false` → redirect to login with error banner;
      with `true` → user created with `default_role` exactly once
      (second login links, not duplicates).
- [x] 1.5 Integration: `GET /api/v1/auth/sessions` lists two parallel
      logins of the same user with distinct ids; `DELETE .../{other}`
      then replaying that session's cookie → 401 on next request;
      deleting another user's session as non-Admin → 403.
- [ ] 1.6 Integration: `trust_idp_mfa = false` → post-callback flow
      enters the existing second-factor challenge; `= true` → shell
      directly.
- [ ] 1.7 Property: no DTO, log line, or audit event in the SSO flow
      contains the client secret plaintext (fuzz random secrets).
- [x] 1.8 CLI E2E: `cli_auth_sessions_list_and_revoke`.
- [ ] 1.9 Web: Sessions card + SSO button render at 360/768/1280 px
      with tokens.css forms; screenshots in PR.

## 2. Domain

- [x] 2.1 Add `sso` types + pure validators under
      `crates/openpanel-domain/src/identity/`; extend
      `SessionRepository` trait with `list_active`, `revoke_other`.

## 3. Application

- [x] 3.1 SQLite migrations (sso_connections, external_identities,
      sso_states) + repo impls.
- [ ] 3.2 `SsoService` + `OIDCProviderPort` impl using the workspace
      `openidconnect` dependency (app layer only).
      > NOTE: an `OpenidConnectAdapter` over reqwest + serde_json
      > ships and passes the mocked-port integration tests, but it
      > deviates from this task: no `openidconnect` crate and no JWKS
      > signature verification (documented in `oidc.rs`). Keep open
      > until the crate is adopted or the deviation is approved.
- [x] 3.3 Session read-model queries + revocation service; middleware
      rejects revoked sessions (additive change).

## 4. Adapters and UI

- [x] 4.1 Routes `/auth/sso/*`, `/api/v1/auth/sessions*`,
      `/api/v1/users/{id}/sessions/{sid}` (Admin guard).
- [x] 4.2 CLI `openpanel auth {sessions,revoke}`.
- [ ] 4.3 Web: login SSO button, Sessions card, Admin per-user view.

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [x] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test against a local Keycloak/test IdP container:
      full login, auto-provision off→on, revoke other session from a
      second browser profile and observe 401.
- [ ] 5.4 Archive with
      `openspec archive refine-identity-with-sso-and-session-control`.

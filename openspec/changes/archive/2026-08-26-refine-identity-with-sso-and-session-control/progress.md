# Progress — refine-identity-with-sso-and-session-control

**Goal:** OIDC login for the panel (authorization-code + PKCE) and
cross-session inventory/revocation.

**Approach:** domain `sso` module (connection VO with ciphertext-layout
validation, external-identity link, single-use login state with pure
callback validation, `OidcPort`/`SsoRepository` traits); SQLite repo +
migration; `SsoService` (owner-only configure with server-side secret
encryption via the AES-256-GCM crypto helpers, begin/callback, mocked
provider port in tests); OIDC adapter implemented directly over reqwest
+ sha2 PKCE S256 + ID-token claim checks (server-to-server TLS token
fetch; JWKS signature verification documented as follow-up);
`SessionRepository::list_for_user`; REST `/auth/sso/*` (public) +
`/api/v1/auth/sso/connection`, `/api/v1/auth/sessions*`,
`/api/v1/users/{id}/sessions/{sid}`; CLI `auth {sessions,revoke}`.

**Done:** 7 domain unit tests (config validation incl. plaintext-secret
rejection, MFA flag interplay, callback mismatch/expiry/nonce,
1000-state uniqueness, identity round-trip); 3 integration tests
(two-session inventory → revoke other → 401 immediately while survivor
works; begin without connection → 503; owner configure encrypts secret
— response redacts it — and begin then reaches discovery → 503).
fmt/clippy clean; full workspace suite passes.

**Update (session 2026-08-26):** fixed callback bug — empty nonce was
passed to validate_callback so every SSO login failed NonceMismatch;
state/expiry now validate via `SsoLoginState::validate_state` and
nonce binding stays with the OIDC port. Task 1.4 done (mocked-Oidc
integration: provision off/on, link-not-duplicate, audits). Task 1.6
done — callback returns `CallbackOutcome::FactorRequired` (existing
pending-login challenge, no session) when the connection distrusts
IdP MFA and the user has an enrolled factor; trusted connections
shell straight through. Task 1.7 done — secret-leak property over
DTOs + audit trail. Task 1.8 done (`cli_auth_sessions_list_and_revoke`).
NOTE task 3.2 kept open: shipped `OpenidConnectAdapter` is reqwest-
based without JWKS verification; openidconnect-crate adoption or
deviation approval pending.

**Remaining (tasks unchecked):** 3.2 (see note), 1.9 web Sessions
card + SSO button, 5.3 Keycloak smoke-test, 5.4 archive.

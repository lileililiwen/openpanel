# Refine Identity with SSO and session control

## Why

Panel login is local-password-only: no OIDC/OAuth2/LDAP path exists
(zero hits across specs and crates), while Coolify ships first-class
OIDC and cPanel ships WebPros SSO — enterprise adopters gate on this.
Session handling is also one-way: the identity spec covers issuing,
expiry, and logout of the *current* session only; users cannot see or
revoke their other sessions, which every peer panel treats as a
security baseline.

## What Changes

- **OIDC login** (authorization-code + PKCE) against a configured
  IdP: discovery, state/nonce validation, optional auto-provisioning
  with a default role, account linking by (issuer, subject).
- **2FA interplay**: configurable whether IdP MFA satisfies the
  factor challenge; otherwise normal second-factor login runs after
  callback. Local password login remains available and unchanged.
- **Session inventory**: list own active sessions with device label,
  IP, created and last-seen timestamps.
- **Session revocation**: revoke any of your own sessions; Admins may
  revoke any user's session; revoked sessions fail at the middleware
  immediately.
- Client secret stored AES-256-GCM encrypted, never returned by any
  surface; feature-flagged off by default.

## Capabilities

### Modified Capabilities

- `identity`: add external-identity login and cross-session control
  alongside the existing user/session/factor model.

## Impact

- Domain: `SsoConnection`, `ExternalIdentity`, `State`/`Nonce` VOs,
  pure `validate_callback`, `SsoError`; `SessionRepository` gains
  `list_active`/`revoke_other`.
- App: `SsoService`, `OIDCProviderPort` + `openidconnect` impl
  (app-layer dep only), SQLite migrations for sso tables, session
  read-model queries.
- API/web: `/auth/sso/{login,callback}`, `/api/v1/auth/sessions*`,
  login-page SSO button, account "Sessions" card.
- Security: secret ciphertext at rest; state rows TTL'd; audit events
  `SsoLogin`, `SessionRevoked{actor, target}`.
- Coupling: identity module only; middleware touch is additive.

## Non-goals

- No LDAP/SAML (follow-up if demand appears).
- No SCIM user sync.
- No per-token device push/kill switch beyond revocation.

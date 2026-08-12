# Add two-factor authentication

## Why

OpenPanel ships argon2id passwords with sessions and RBAC, but a single
factor is the single biggest Baota-class weakness: a stolen
administrator password gives the attacker the box. cPanel, Cloudflare,
and every modern panel support TOTP at minimum, and WebAuthn where
browsers allow. This change adds an Owner/Admin/User opt-in second
factor so a panel credential alone is never sufficient for a privileged
login. Pure-Rust crypto, no PHP, no pluggable auth chain — a deliberate
fit for the "compiled language, no runtime script injection" pitch.

## What Changes

- New `identity` capabilities for enrolling, verifying, and revoking
  TOTP and WebAuthn factors tied to a user account.
- Login flow gains a "second factor required" state when the account
  has at least one enrolled factor; the panel never accepts a
  password-only login for those accounts.
- Recovery codes generated once at enrollment; consumption decremented
  and audited.
- `remember_device` produces a short-lived, per-factor, signed cookie
  so a verified browser does not re-prompt on every login; the cookie
  is scoped to the factor and the device fingerprint.
- Audit events for every factor event: enrolled, used, failed,
  revoked, recovery consumed.
- REST, CLI, and `/settings/security` web surface.

## Capabilities

### Modified Capabilities

- `identity`: password authentication gains an optional second factor
  bound to the user account.

## Impact

- Domain: `Factor` enum (`Totp`, `WebAuthn`), `RecoveryCodeSet`,
  `RememberedDevice` value objects, `TotpSecret`, `WebAuthnCredential`
  with the registered credential ID, public key, counter, transports.
- App: `TwoFactorService` use cases, SQLite migrations, audit.
- API/CLI/web: enrollment endpoints, second-factor challenge route,
  device-remember toggle, `/settings/security` UI.
- Crypto: `totp-rs` crate (HMAC-SHA1 RFC 6238), `webauthn-rs` crate
  (ES256 / RS256 attestation, constant-time signature verify).
- Configuration: issuer label, challenge lifetime (default 5 min),
  allowed WebAuthn origins, recovery-code count (default 10), and
  remember-device lifetime (default 30 days).

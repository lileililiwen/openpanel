# Add two-factor authentication — Design

## Domain model

```
User
  └─ Factor (one or many)
        ├─ TOTP   { secret: EncryptedAtRest, digits=6, period=30, algo=SHA1,
        │           verified_at, last_used_at, revoked_at? }
        └─ WebAuthn { credential_id, public_key_spki, sign_count,
                      transports, uv=Preferred, verified_at, last_used_at,
                      revoked_at? }
  └─ RecoveryCodeSet { bcrypt_hash_set[N], remaining: u8 }
  └─ RememberedDevice { factor_id, cookie_token_hash, expires_at,
                        ua_hash, ip_prefix }
```

- TOTP secret is stored encrypted at rest under the panel master key
  (same envelope as DB passwords and TLS keys).
- WebAuthn public keys live in the DB unencrypted (they are public);
  the credential ID is the user-visible handle.
- `sign_count` is incremented on every successful WebAuthn assertion;
  a counter regression rejects the assertion (clone detection).

## Login flow

```
POST /api/v1/identity/login  { username_or_email, password }
   ├─ invalid  → 401
   ├─ valid, no enrolled factor → 200 + session cookie
   └─ valid, has factor  → 200 { status: "factor_required", challenge_id }
                               Set-Cookie: openpanel_challenge=<opaque>
POST /api/v1/identity/login/factor
     { challenge_id, kind, response }
   ├─ TOTP  { code }        → verify ±1 step window
   ├─ WebAuthn { assertion } → finish WebAuthn ceremony
   └─ Recovery { code }     → bcrypt-compare, decrement
   └─ success  → 200 + session cookie
       optionally Set-Cookie: openpanel_2fa_remember=<signed>
   └─ failure  → audit, 401
```

A challenge is single-use, lifetime 5 minutes, and stored in the DB
hashed. Replays fail with a redacted audit event.

## Recovery codes

- Generated at first factor enrollment: 10 single-use codes,
  base32-encoded (Crockford alphabet), shown to the user exactly once.
- Stored as independent bcrypt hashes; the plaintext never persists.
- A user with no usable factor and no remaining recovery codes MUST
  contact an Owner for reset; there is no out-of-band backdoor.

## WebAuthn

- Attestation: `None` (we don't need to ship a trust store; the panel
  is the relying party).
- Algorithms: ES256, RS256, EdDSA.
- Resident keys (`uv=Preferred`); `rk=false` is rejected so platform
  authenticators cannot be evicted by browser cleanup.
- Origins whitelist from `OPENPANEL__IDENTITY__WEBAUTHN_ORIGINS`,
  default `https://<panel-host>`.

## Audit

```
TwoFactorEnrolled   { user_id, factor_kind }
TwoFactorVerified   { user_id, factor_kind }
TwoFactorFailed     { user_id, factor_kind, reason }
TwoFactorRevoked    { user_id, factor_kind, actor_user_id }
RecoveryCodeConsumed{ user_id, remaining }
DeviceRemembered    { user_id, factor_kind }
```

## Tests (TDD)

```
1.1  Domain unit tests for Factor/TOTP/WebAuthn/recovery constructors
     and invariants (digits, period, counter monotonicity).
1.2  Property: any TOTP code outside ±1 window is rejected;
     WebAuthn counter regression is rejected; recovery codes are
     single-use.
1.3  TwoFactorService tests with mock clock, repo, audit
     (enrollment happy path, recovery consumption, replay of
     consumed recovery code).
1.4  Integration: login → factor_required → TOTP success;
     login → factor_required → wrong TOTP → audit; WebAuthn
     ceremony happy path; remember-device cookie skips the
     second factor on the next login.
1.5  CLI E2E: enroll TOTP via `openpanel user 2fa enroll totp`,
     list factors, revoke; verify recovery code consumption.
1.6  Web integration: /settings/security page renders enrolled
     factors, recovery-code banner, and CSRF on enroll/revoke.
```

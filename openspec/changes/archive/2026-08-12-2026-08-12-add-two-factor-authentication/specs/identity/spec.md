## ADDED Requirements

### Requirement: Factor Enrollment and Lifecycle

The system SHALL let a user enroll one or more TOTP and WebAuthn factors against their own account. Enrolled factors are scoped per user; only the owner or an Owner-role actor may revoke. TOTP secrets SHALL be stored encrypted at rest under the panel master key; WebAuthn credential public keys are stored unencrypted but the private key never leaves the authenticator.

#### Scenario: Enroll TOTP

- **WHEN** an authenticated user submits a TOTP enrollment request
- **THEN** the panel returns a base32 secret, provisioning URI, and a verification step that rejects unverified factors.

#### Scenario: Enroll WebAuthn

- **WHEN** an authenticated user submits a WebAuthn registration ceremony
- **THEN** the panel validates the attestation, persists the public key and credential ID, and rejects ceremonies whose origin is not allowlisted.

#### Scenario: Owner revokes a factor

- **WHEN** an Owner revokes a factor belonging to another user
- **THEN** the factor is marked revoked, the audit log records the actor, and the affected user is forced through factor-required login again.

### Requirement: Second-Factor Login

A login that authenticates a user with one or more enrolled factors SHALL NOT issue a session until a valid second factor is presented. The challenge is single-use, time-bounded (default 5 minutes), stored hashed, and never returned after consumption.

#### Scenario: Password-only login rejected for 2FA user

- **WHEN** a user with an enrolled factor submits a valid password
- **THEN** the response is `factor_required` with a challenge ID and no session cookie is set.

#### Scenario: TOTP succeeds

- **WHEN** the user submits a TOTP code within ±1 window of the current step
- **THEN** the panel issues a session cookie, records a `TwoFactorVerified` audit event, and never returns the verified code again.

#### Scenario: WebAuthn counter regression

- **WHEN** the WebAuthn assertion's stored sign_count is greater than or equal to the presented counter
- **THEN** the assertion is rejected and a `TwoFactorFailed` audit event is recorded.

#### Scenario: Replayed challenge

- **WHEN** a challenge ID that has already been consumed is presented
- **THEN** the panel returns 401 and audits the replay attempt without revealing the original user.

### Requirement: Recovery Codes

At first factor enrollment the panel SHALL generate a configurable number of single-use recovery codes (default 10), show them to the user exactly once, and store only independent bcrypt hashes. Consuming a code reduces the remaining count by one; a user with no remaining codes and no usable factor MUST contact an Owner.

#### Scenario: Consume a recovery code

- **WHEN** the user submits a previously shown recovery code during the factor challenge
- **THEN** the panel marks the code consumed, decrements `remaining`, and completes the login.

#### Scenario: Reuse a consumed code

- **WHEN** the user submits a recovery code that was already consumed
- **THEN** the panel rejects the login and records the attempt.

### Requirement: Remember-Device Cookie

The system SHALL support issuing a short-lived, per-factor, signed `openpanel_2fa_remember` cookie after successful second-factor verification when the user requests it. The cookie is scoped to the factor and a user-agent + IP-prefix fingerprint. Subsequent logins that present a valid cookie skip the factor challenge for that factor on that device.

#### Scenario: Remember-device skips factor

- **WHEN** a user logs in from a browser that previously presented a valid remember-device cookie for the matched factor
- **THEN** the panel issues the session cookie without re-prompting.

#### Scenario: Tampered cookie

- **WHEN** the remember-device cookie's signature does not validate
- **THEN** the panel falls back to factor-required and audits the tamper attempt.

### Requirement: Second-Factor Surfaces

REST, CLI, and `/settings/security` web surfaces SHALL support factor list, enroll, verify-enrollment, revoke, and recovery-code regenerate. Browser mutations MUST enforce CSRF. The panel SHALL NEVER return stored factor secrets, recovery codes (after the once-on-enrollment view), or private key material in any response.

#### Scenario: User lists their factors

- **WHEN** an authenticated user requests `GET /api/v1/identity/factors`
- **THEN** only that user's factors are returned, with `kind`, `enrolled_at`, `last_used_at`, and `revoked_at`.

#### Scenario: Web enroll TOTP

- **WHEN** the user submits the enroll-TOTP form on `/settings/security`
- **THEN** the secret is shown once, the form asks for a verification code, and CSRF is enforced.

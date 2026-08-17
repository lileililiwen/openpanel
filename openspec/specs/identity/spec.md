# identity Specification

## Purpose
TBD - created by archiving change bootstrap-ddd-architecture. Update Purpose after archive.
## Requirements
### Requirement: User Aggregate

The identity context SHALL model a `User` aggregate with fields `id`
(UUID v4), `username` (unique, 3-32 chars, alphanumeric+`-_.`), `email`
(unique, validated as RFC 5321), `password_hash` (argon2id, never
returned to callers), `role` (one of `Owner`, `Admin`, `User`),
`parent_account_id` (nullable user reference for the reseller
hierarchy change), `hosting_plan_id` (nullable `HostingPlanId` for
the hosting-plan change), `created_at`, `disabled_at` (nullable),
`last_login_at` (nullable). Plaintext passwords MUST NOT appear in
any log, error message, or API response.

#### Scenario: Backfilled users have NULL on both fields

- **WHEN** the v005 migration runs on an existing DB
- **THEN** every existing row has `parent_account_id IS NULL AND hosting_plan_id IS NULL`.

### Requirement: Parent Account and Hosting Plan Fields

The User aggregate SHALL record two optional identity references: `parent_account_id: Option<UserId>` (used by the reseller hierarchy change) and `hosting_plan_id: Option<HostingPlanId>` (used by the hosting-plan change). Both SHALL be nullable. Self-parenting SHALL be rejected by the user constructor with `IdentityError::ParentAccountCycle`. The repository trait SHALL expose placeholder methods `find_children(parent_id)` and `find_by_plan(plan_id)` that return empty until the corresponding follow-on changes ship.

#### Scenario: Create a user with both fields

- **WHEN** an Owner creates a user with `parent_account_id=u1, hosting_plan_id=p1`
- **THEN** the user is persisted and both fields round-trip through SQLite.

#### Scenario: Self-parenting rejection

- **WHEN** an Owner creates a user with `parent_account_id=u_new.id`
- **THEN** creation fails with `IdentityError::ParentAccountCycle` and no row is written.

### Requirement: Repository Lookup Helpers (Placeholders)

The `UserRepository` trait SHALL expose `find_children(parent_id)` and `find_by_plan(plan_id)`. Their behaviour is deliberately empty until the follow-on changes ship; they MUST be safe to call.

#### Scenario: Helper called before follow-on changes land

- **WHEN** a caller invokes either placeholder
- **THEN** it returns an empty list and does not error.

#### Scenario: Backed by indexed columns

- **WHEN** the v005 migration is complete and helpers are re-implemented by the follow-on changes
- **THEN** the indexes `idx_users_parent_account_id` and `idx_users_hosting_plan_id` are used by the queries.

#### Scenario: Creating a user

- **WHEN** an Owner submits a valid username, email, role, and password
  (≥ 12 chars)
- **THEN** the user is persisted with a unique id, an argon2id hash,
  `created_at = now()`, and the plaintext password is discarded.

#### Scenario: Rejected weak password

- **WHEN** a user creation request provides a password shorter than 12
  characters
- **THEN** the application returns `IdentityError::PasswordTooShort`
  and no user record is created.

### Requirement: Role-Based Access Control

The identity context SHALL enforce three roles with the following
capabilities:

- **Owner** — all operations including user creation, role changes, and
  deletion.
- **Admin** — manage sites/databases/files for assigned users; cannot
  create other users or change roles.
- **User** — manage only resources owned by their own account.

#### Scenario: Non-owner denied user creation

- **WHEN** an Admin attempts `POST /api/v1/identity/users`
- **THEN** the API returns `403 Forbidden` with code `forbidden`.

#### Scenario: Owner can change roles

- **WHEN** an Owner submits `PATCH /api/v1/identity/users/{id}` with
  `{"role": "admin"}`
- **THEN** the user's role is updated and an audit event
  `role_changed` is recorded.

### Requirement: Session Lifecycle

The identity context SHALL issue opaque random session tokens (256 bits,
base64url) stored hashed in the database. Sessions SHALL expire after
24 hours of inactivity or 7 days of absolute lifetime, whichever comes
first. Logout SHALL invalidate the session immediately.

#### Scenario: Successful login

- **WHEN** a user posts valid credentials to `POST /api/v1/identity/login`
- **THEN** the API returns the session token in JSON and sets the
  `openpanel_session` HttpOnly, Secure, SameSite=Lax cookie; a row is
  inserted in `sessions`; an audit event `login_success` is recorded;
  `last_login_at` is updated on the user.

#### Scenario: Failed login

- **WHEN** a user posts invalid credentials
- **THEN** the API returns `401` with code `invalid_credentials`,
  records an audit event `login_failure`, and DOES NOT reveal whether
  the username or password was wrong.

#### Scenario: Logout

- **WHEN** an authenticated user posts `POST /api/v1/identity/logout`
- **THEN** the session row is deleted, the cookie is cleared, and an
  audit event `logout` is recorded.

#### Scenario: Expired session rejected

- **WHEN** a request carries a session token whose row's `expires_at`
  is in the past
- **THEN** the middleware returns `401` with code `session_expired` and
  deletes the session row.

### Requirement: Password Hashing

The identity context SHALL hash passwords with argon2id using the
parameters `m_cost=19456, t_cost=2, p_cost=1` (OWASP 2024 minimum). The
hash SHALL include a per-user salt and SHALL be stored in PHC string
format. Plaintext passwords MUST NOT be retained after hashing.

#### Scenario: Hash verification

- **WHEN** a user attempts login with the correct password
- **THEN** `Argon2::verify_password(&hash, &submitted_password)` returns
  `Ok(())` within 250ms on a modern CPU.

### Requirement: CLI and API Surface

The identity context SHALL expose:

- CLI: `openpanel user create|list|disable|delete`, `openpanel serve`,
  `openpanel migrate`.
- HTTP: `POST /api/v1/identity/login`, `POST /api/v1/identity/logout`,
  `GET /api/v1/identity/me`, `GET /api/v1/identity/users`,
  `POST /api/v1/identity/users`, `PATCH /api/v1/identity/users/{id}`,
  `DELETE /api/v1/identity/users/{id}`.

#### Scenario: List users excludes hashes

- **WHEN** any authenticated user calls `GET /api/v1/identity/users`
- **THEN** the response contains user records WITHOUT `password_hash`
  fields.

#### Scenario: Disabling a user

- **WHEN** an Owner calls `POST /api/v1/identity/users/{id}/disable`
- **THEN** `disabled_at` is set to now, the user cannot log in
  thereafter, and an audit event `user_disabled` is recorded.

#### Scenario: Disabled user cannot log in

- **WHEN** a disabled user posts valid credentials
- **THEN** the API returns `401` with code `account_disabled` and an
  audit event `login_failure` is recorded.

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


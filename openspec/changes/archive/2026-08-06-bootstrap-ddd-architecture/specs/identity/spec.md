## ADDED Requirements

### Requirement: User Aggregate

The identity context SHALL model a `User` aggregate with fields `id`
(UUID v4), `username` (unique, 3-32 chars, alphanumeric+`-_.`), `email`
(unique, validated as RFC 5321), `password_hash` (argon2id, never
returned to callers), `role` (one of `Owner`, `Admin`, `User`),
`created_at`, `disabled_at` (nullable), `last_login_at` (nullable).
Plaintext passwords MUST NOT appear in any log, error message, or API
response.

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
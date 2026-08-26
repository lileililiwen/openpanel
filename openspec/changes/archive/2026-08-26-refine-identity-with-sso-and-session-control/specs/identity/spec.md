## ADDED Requirements

### Requirement: OIDC Login

The system SHALL support panel login via an OpenID Connect provider
using the authorization-code flow with PKCE. State and nonce SHALL be
validated; local password login SHALL remain available and unchanged.

#### Scenario: First SSO login with auto-provision

- **WHEN** an unlinked identity authenticates against a connection
        with `auto_provision = true`
- **THEN** a user is created with the connection's `default_role`, an
        `ExternalIdentity(issuer, subject)` link is stored, and a
        session is issued.

#### Scenario: Auto-provision disabled

- **WHEN** an unlinked identity authenticates with
        `auto_provision = false`
- **THEN** login is refused with `SsoError::ProvisionDisabled` and no
        user or link is created.

#### Scenario: Replay rejected

- **WHEN** the callback's state does not match an outstanding,
        unexpired state row
- **THEN** login fails with `SsoError::StateMismatch` and no session
        is issued.

### Requirement: Second-Factor Interplay

An SSO-authenticated session SHALL satisfy the second-factor challenge
only when the connection sets `trust_idp_mfa`; otherwise the existing
factor challenge SHALL run after callback before shell access.

#### Scenario: IdP MFA trusted

- **WHEN** `trust_idp_mfa = true` and the IdP asserts an MFA claim
- **THEN** the resulting session reaches the shell without a further
        factor prompt.

#### Scenario: IdP MFA not trusted

- **WHEN** `trust_idp_mfa = false`
- **THEN** the user is routed into the standard second-factor login.

### Requirement: Session Inventory

An authenticated user SHALL be able to list their own active sessions
with device label, IP address, creation time, and last-seen time;
timestamps of other users' sessions SHALL NOT be exposed to them.

#### Scenario: Own sessions listed

- **WHEN** a user with two live sessions calls
        `GET /api/v1/auth/sessions`
- **THEN** exactly two entries are returned, each with device label,
        IP, created_at, last_seen_at.

### Requirement: Session Revocation

A user SHALL be able to revoke any of their own sessions; an Admin
SHALL be able to revoke any user's session. A revoked session SHALL be
rejected by the auth middleware from its next use onward.

#### Scenario: Revoke other device

- **WHEN** session B of user U is revoked while U holds session A
- **THEN** requests bearing B's cookie fail 401 immediately and A
        continues to work.

#### Scenario: Admin revokes user session

- **WHEN** an Admin revokes any session of user U
- **THEN** that session is rejected on next use and audit event
        `SessionRevoked{actor, target}` is recorded.

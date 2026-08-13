## ADDED Requirements

### Requirement: Personal Access Token Lifecycle

The system SHALL let a user create, list, rotate, and revoke personal access tokens bound to their own account. Plaintext tokens are shown to the creator exactly once at creation or rotation; only an HMAC-SHA256 hash with a server pepper is persisted. Tokens SHALL have a label, an explicit scope set, an explicit expiration, an optional CIDR allowlist, and a revocation state.

#### Scenario: Create a token

- **WHEN** an authenticated user posts `{ label, scopes, expires_at }`
- **THEN** the panel persists the token, returns the plaintext exactly once with a `shown_once` flag, and audits `TokenCreated`.

#### Scenario: Rotate a token

- **WHEN** an authenticated user rotates an active token
- **THEN** the panel revokes the old token atomically, issues a new token with the same scopes, returns the new plaintext exactly once, and audits `TokenRotated`.

#### Scenario: Re-show is impossible

- **WHEN** a user requests `GET /api/v1/identity/tokens/{id}`
- **THEN** the response contains metadata only and never the plaintext or hash.

### Requirement: Bearer Authentication

The system SHALL accept `Authorization: Bearer openpanel_pat_…` on every API route that accepts session cookies. The bearer middleware MUST resolve the token to a principal, enforce scope, expiration, and CIDR allowlist before the route handler runs, and MUST record the token ID (never the token itself) in every audit event for the request.

#### Scenario: Bearer works

- **WHEN** a request presents a valid bearer for a route within the token's scope
- **THEN** the route handler runs under the token's principal and the audit log records `token_id`.

#### Scenario: Scope mismatch

- **WHEN** a request presents a valid bearer for a route outside the token's scope
- **THEN** the panel returns 403 and audits `TokenScopeRejected`.

#### Scenario: Expired token

- **WHEN** a request presents a bearer whose `expires_at` is in the past
- **THEN** the panel returns 401 and audits `TokenExpired`.

#### Scenario: CIDR rejected

- **WHEN** a request presents a bearer whose CIDR allowlist does not contain the peer address
- **THEN** the panel returns 403 and audits `TokenCidrRejected`.

### Requirement: Per-Token Rate Limit

Every bearer-authenticated request SHALL be subject to a per-token token-bucket rate limit. A token that exceeds the burst capacity receives `429 Retry-After: <seconds>` and the panel audits `TokenRateLimited`.

#### Scenario: Burst exhausted

- **WHEN** a token makes more requests than the configured burst within one second
- **THEN** the next request returns 429 with a `Retry-After` header.

### Requirement: Scopes and RBAC for Tokens

Scopes SHALL be strings of `<bounded_context>:<verb>`; a route handler MUST declare the scopes it accepts. A user SHALL only self-issue scopes at or below their own role's grant set. An Owner may issue tokens on behalf of any user; Admins may issue tokens on behalf of non-Owner users.

#### Scenario: User self-issues read-only

- **WHEN** a User-role actor requests a token with `sites:write`
- **THEN** creation fails with 403.

#### Scenario: Owner issues a token for any user

- **WHEN** an Owner creates a token with `cron:run` for an Admin
- **THEN** creation succeeds and the token is scoped to that Admin's principal.

### Requirement: Token Surfaces

REST, CLI, and `/settings/tokens` web surfaces SHALL support create, list, revoke, and rotate. Browser mutations MUST enforce CSRF. The web UI MUST display the plaintext exactly once and SHALL never re-display it after the page is rendered.

#### Scenario: Web rotate

- **WHEN** the user clicks Rotate on `/settings/tokens`
- **THEN** the new plaintext is shown once on the next page and the old token's `revoked_at` is set immediately.

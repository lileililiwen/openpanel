# Add scoped API tokens

## Why

Baota, cPanel, and every commercial panel have an automation problem:
scripts use either (a) the owner password, which leaks via logs and
CI, or (b) a long-lived cookie, which is unbounded. OpenPanel currently
has no non-interactive credential. This change introduces first-class
**API tokens**: scoped, expiring, auditable bearer credentials that
replace password reuse for automation. Tokens are stored hashed at
rest, returned to the caller exactly once at creation, and presented
to the API via a header (never a cookie). This is the canonical
mitigation for "I need to curl the panel from cron".

## What Changes

- New `api-tokens` bounded context with a `Token` aggregate carrying
  owner, label, scope set, allowed CIDR allowlist (optional),
  expiration, last-used timestamp, and revocation state.
- Token plaintext is shown to the creator exactly once (at creation
  and at one-time rotation). Only a hash is persisted.
- Authorization layer recognises a bearer header
  (`Authorization: Bearer openpanel_pat_…`) as equivalent to a
  session-bound principal, scoped to the token's allowed routes and
  verbs.
- Per-token rate limits (token bucket) so a leaked token cannot
  exhaust host resources.
- REST, CLI, and `/settings/tokens` web surface.

## Capabilities

### New Capabilities

- `api-tokens`: personal access token lifecycle and bearer auth.

### Modified Capabilities

- `identity`: bearer authentication is accepted alongside session
  cookies for every API route, with scope and rate limits enforced
  per token.

## Impact

- Domain: `ApiToken` aggregate, `TokenScope`, `TokenFormat`
  (`openpanel_pat_<base32>`), `TokenHash` (HMAC-SHA256 over a server
  pepper).
- App: `ApiTokenService`, `BearerAuthenticator` middleware port,
  rate-limit store, audit.
- API/CLI/web: `POST/GET/DELETE /api/v1/identity/tokens`,
  `openpanel token {create,list,revoke,rotate}`, `/settings/tokens`.
- Configurable: default scopes per role, default lifetime (90 days),
  hard max lifetime (365 days), rate limit (default 60 req/min),
  allowed CIDR allowlist.

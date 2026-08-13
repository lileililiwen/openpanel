# Add scoped API tokens — Design

## Token format

```
openpanel_pat_<24-char base32 random>.<4-char checksum>
```

- The body is a 120-bit CSPRNG value (`rand::rngs::OsRng`).
- The checksum is the first 20 bits of `SHA256(body)` rendered as four
  base32 characters, allowing typo detection without a DB lookup.
- The stored credential is `HMAC-SHA256(pepper, body)`. The pepper
  is the panel master key; a stolen DB does not yield usable tokens.

## Storage

```sql
CREATE TABLE api_tokens (
  id              TEXT PRIMARY KEY,           -- ULID
  user_id         TEXT NOT NULL,
  label           TEXT NOT NULL,
  hash            BLOB NOT NULL,              -- HMAC-SHA256(body)
  scopes          TEXT NOT NULL,              -- JSON array
  cidr_allowlist  TEXT,                       -- JSON array or NULL
  created_at      INTEGER NOT NULL,
  expires_at      INTEGER NOT NULL,
  last_used_at    INTEGER,
  revoked_at      INTEGER
);
```

- Plaintext body and checksum are never persisted.
- A successful `Authorization: Bearer …` header is hashed, looked up
  in O(log n), then verified by `constant_time_eq`.

## Scopes

Scopes are strings of the form `<bounded_context>:<verb>`, e.g.
`sites:read`, `sites:write`, `databases:read`, `cron:run`. A token
without a matching scope for the requested route returns 403. Admin
and Owner roles may issue tokens with their own scopes plus any
subset of their role's scopes; Users may only self-issue `read` scopes.

## Rate limit

- Per-token token bucket; capacity `OPENPANEL__API__RATE_BURST`
  (default 60), refill `OPENPANEL__API__RATE_PER_MIN` (default 60).
- Excess returns `429 Retry-After: <seconds>` and audits a
  `TokenRateLimited` event.
- Buckets live in-memory per process; multi-process deployments
  accept slightly looser limits (documented).

## Endpoints

```
POST   /api/v1/identity/tokens
       { label, scopes, expires_at, cidr_allowlist? }
       → 201 { id, plaintext_token: "openpanel_pat_….<cksum>",
                plaintext_token_shown_once: true }
GET    /api/v1/identity/tokens              list (no plaintext)
DELETE /api/v1/identity/tokens/{id}         revoke
POST   /api/v1/identity/tokens/{id}/rotate  issues a new token;
                                            old token revoked atomically;
                                            new plaintext shown once
```

## Bearer middleware

```
Authorization: Bearer openpanel_pat_…
  → lookup by HMAC hash → load scopes
  → if CIDR allowlist set, reject if request peer not in any prefix
  → if expires_at < now, reject
  → check route ∈ scopes; else 403
  → rate-limit bucket; 429 if exhausted
  → attach principal = (user_id, token_id) for audit
```

Audit uses the same `AuditService`; every API request that is
authenticated by a token records the token ID instead of session ID.

## Tests

```
1.1  Unit: format/checksum, hash deterministic with pepper,
     scope parser, CIDR matcher.
1.2  Property: random body always produces a unique hash
     (proptest 1000); invalid checksums never match a real body.
1.3  Service tests with mock repo and audit covering create,
     list, revoke, rotate, and rate-limit exhaustion.
1.4  Integration: bearer happy path, expired token 401, scope
     mismatch 403, CIDR rejection, 429 on burst.
1.5  CLI E2E: create → curl → revoke → 401.
1.6  Web: /settings/tokens list/create/revoke UI with CSRF;
     plaintext shown-once banner.
```

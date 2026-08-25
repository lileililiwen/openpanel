# Refine Identity with SSO and session control — Design

## Explore & Reuse

- `crates/openpanel-domain/src/identity/session.rs:67` — `Session`
  entity (add `last_seen_at`, `user_agent_label` read-model fields at
  app layer to avoid widening the aggregate).
- `crates/openpanel-domain/src/identity/repository.rs:72` —
  `SessionRepository` trait: add `list_active`, `revoke_other`;
  SQLite impl follows existing method patterns.
- `crates/openpanel-api/src/middleware/session.rs` — single choke point
  where revoked sessions must start failing (it already loads the
  session per request).
- Factor enrollment precedent for "second credential type attached to
  a user" (`Factor` lifecycle, `factor.rs:165` revoke pattern) shapes
  `ExternalIdentity`.
- Secret-at-rest: AES-256-GCM `crypto.rs` layout for the OIDC client
  secret; never returned by DTOs (same rule as TLS keys).
- Login UI: `crates/openpanel-web/src/login.rs` is the canonical
  styled-form example; add SSO button + sessions card using tokens.css.
- New dependency: `openidconnect` crate, workspace-level, referenced
  only from openpanel-app (adapters may not import it).

## Domain additions (pure)

```rust
pub struct SsoConnection { issuer_url, client_id, secret_cipher: String,
                           default_role: Role, auto_provision: bool }
pub struct ExternalIdentity { issuer: IssuerUrl, subject: Subject, user_id: UserId }
pub enum SsoError { Discovery, StateMismatch, ProvisionDisabled, LinkConflict }

// pure helpers, unit-tested without network:
pub fn new_login_state(now) -> (State, Nonce, PkceVerifier);
pub fn validate_callback(state_sent, state_got, nonce_sent, id_nonce, now)
    -> Result<(), SsoError>;
```

## Flow

```
GET /auth/sso/login
  generate (state, nonce, verifier); store state row w/ 10-min TTL
  redirect to provider auth endpoint (PKCE S256)
GET /auth/sso/callback?code&state
  validate state; exchange code (OIDCProviderPort); verify nonce
  find ExternalIdentity(issuer, subject)
    | found    -> issue Session (existing primitives)
    | missing  -> auto_provision? create User{role=default_role}
               : reject ProvisionDisabled
  audit SsoLogin; redirect to shell
```

2FA interplay: an SSO-authenticated session is marked
`mfa_satisfied = true` when the connection sets
`trust_idp_mfa = true`; otherwise the normal factor challenge runs
after callback (reuse second-factor login machinery).

## Sessions endpoints

```
GET    /api/v1/auth/sessions            -> own active sessions
DELETE /api/v1/auth/sessions/{id}      -> own (404 unless owner of session)
DELETE /api/v1/users/{id}/sessions/{sid} -> Admin only
```

Revocation = set `revoked_at`; middleware rejects on next use; current
session revocation behaves like logout.

## Config

```toml
[identity.sso]
enabled       = false
issuer_url    = ""
client_id     = ""
client_secret = ""        # encrypted at rest after first write
default_role  = "user"
auto_provision = false
trust_idp_mfa  = false
```

## Layering

Domain: VOs + pure validation + trait extensions (no HTTP concepts).
App: `SsoService`, `OIDCProviderPort` impl, repo impls. API/web/CLI:
adapters. Composition root unchanged (identity module grows routes).

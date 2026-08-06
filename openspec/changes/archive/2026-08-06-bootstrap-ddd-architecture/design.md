# Design: Bootstrap DDD Architecture

## Context

OpenPanel is a Rust workspace at `/home/paul/code/openpanel` with five
crates declared in `Cargo.toml`: `openpanel-core`, `openpanel-domain`,
`openpanel-app`, `openpanel-agent`, `openpanel-cli` (plus empty
placeholder dirs `openpanel-app` and `openpanel-domain` for monitoring
and domain-side monitoring). The crates have **no source code yet**.
Two empty subdirectories under `openpanel-domain` (`hosting/`,
`monitoring/`, `common/`, `identity/`) and two under `openpanel-app`
(`monitoring/`, `identity/`) hint at the intended DDD layering but no
contracts are written down.

The goal of this change is to make the layering enforceable so the
five MVP bounded contexts (identity, sites, ssl, databases, files) can
land in follow-on changes without re-litigating the structure. Every
choice below is sized for a single-host bundled deployment (per the
"Deployment" decision in the project context).

## Goals / Non-Goals

**Goals:**

- Make the DDD layer boundaries mechanically checkable via `cargo`'s
  dependency graph (`openpanel-domain` MUST NOT link `sqlx`/`axum`).
- Make adding a new bounded context a mechanical, copy-paste operation
  bounded to a single `register()` call.
- Ship a working identity/auth path so subsequent feature changes can
  be developed against real users and sessions.
- Establish config layering as the only way to override settings — no
  `std::env::var` calls scattered through the codebase.

**Non-Goals:**

- Sites, SSL, databases, files features — each lands in its own follow-on
  change after this one is archived.
- Multi-host / distributed agent — explicitly deferred.
- WASM plugin runtime — explicitly deferred.
- PostgreSQL — SQLite only in v0.1; the schema allows adding Postgres
  later via a different `DatabaseDriver` impl.
- Frontend UI work — `web/` is left for a separate change; this change
  only emits JSON contracts.

## Decisions

### 1. Layer boundaries enforced by `Cargo.toml` (not traits)

**Decision**: Each layer is its own crate. Dependency direction is
enforced by `[dependencies]` in each `Cargo.toml`.

**Rationale**: A trait cannot prevent someone from importing
`openpanel_api::Router` into `openpanel-domain`. A crate boundary is
a hard compile-time wall. If `openpanel-domain/Cargo.toml` doesn't list
`sqlx`, it cannot accidentally use it.

**Alternatives considered**:

- Single crate with module hierarchy — easier to start, but the
  project will outgrow it; Cargo workspaces are free.
- Macro-enforced layering (e.g. `cargo-deny` or custom lints) — adds
  tooling overhead for a benefit that crate boundaries already provide.

### 2. Module trait registry, not free-form DI

**Decision**: A single `Module` trait lives in `openpanel-core`; each
bounded context implements it exactly once. The composition root calls
`app.register(IdentityModule)`.

**Rationale**: One trait is enough to express "I'm a module — here's my
routes, CLI, migrations, config schema." Avoiding a full DI framework
keeps the dependency surface small and the debug path obvious. The
trait is object-safe where it needs to be (`dyn Module` for
heterogeneous storage in the registry) and value-generic where it
needs to be (for compile-time config schemas later).

**Alternatives considered**:

- `inventory` collect!() based plugin registration — too magical for a
  codebase that values inspectability.
- Trait-per-concern (separate traits for routes, CLI, migrations) —
  more boilerplate per module; `Module` aggregates them.

### 3. Config layering with `figment` + `jsonschema`

**Decision**: Use `figment` to merge defaults → `/etc/openpanel` →
`./openpanel.toml` → env vars (`OPENPANEL__` prefix) → CLI flags.
After merging, validate the result against the merged JSON Schema
produced by each module's `config_schema()`.

**Rationale**: `figment` already handles the merge order and the
env-var-with-double-underscore convention. `jsonschema` gives us
per-field error paths. We don't need a custom config language.

**Alternatives considered**:

- `config` crate — already in workspace deps; `figment` is simpler for
  this layering and has explicit env-var support.
- Hand-rolled merger — too much code for what figment provides.

### 4. SQLite + `sqlx` for v0.1, pluggable later

**Decision**: SQLite via `sqlx` (already in workspace deps) is the only
database for v0.1. Repositories take a `Box<dyn DatabaseDriver>` so a
Postgres driver can be added later.

**Rationale**: SQLite is zero-ops for a single-host panel; it satisfies
the 99% case. `sqlx` gives us compile-time SQL checking with the
`query!` macro.

**Trade-off**: No native enums/arrays; we model `role` as `TEXT`
checked by code, not by the database. Acceptable for v0.1.

### 5. argon2id for password hashing

**Decision**: Use `argon2` crate with OWASP 2024 minimum parameters
(`m=19456, t=2, p=1`). Reject passwords < 12 characters in the
domain `Password` value object.

**Rationale**: argon2id is the consensus recommendation. 12-character
minimum is the current NIST guidance. Hashing belongs in the domain
layer (the `Password` value object) so application code can never
receive a plaintext password.

### 6. Session storage: server-side sessions, opaque tokens

**Decision**: Sessions are rows in `sessions` table; tokens are 256-bit
random, base64url-encoded, stored as argon2id hashes (same as passwords).
The token is sent to the client in JSON **and** set as an HttpOnly
cookie. Server-side sessions enable revocation (logout deletes the row).

**Rationale**: JWTs cannot be revoked without a blacklist, which is
just a server-side session by another name. Opaque tokens + server
state is simpler and more secure for an admin panel.

### 7. Out-of-process agent is the same binary

**Decision**: `openpanel-agent` is a separate crate but the production
deployment runs `openpanel serve` which embeds the agent's background
tasks. `openpanel-agent` is also published as a standalone binary for
multi-host deployments later.

**Rationale**: Lets the codebase be ready for multi-host without forcing
it now. The agent owns the SQLite pool and the background task
supervisor; the API consumes it via trait.

### 8. Audit table belongs to the architecture, not identity

**Decision**: The `audit_log` table schema is owned by the
architecture layer (it appears in `migrations/000_audit.sql`), but
identity is the first context to write to it via `AuditService`.

**Rationale**: Multiple contexts will write audit events. If each
context owned its own audit table, cross-context queries would be
nightmare. One table, one schema, one writer trait.

## Risks / Trade-offs

- **Risk**: The `Module` trait becomes too large as more concerns are
  added → *Mitigation*: split into smaller traits (`HasRoutes`,
  `HasCli`, `HasMigrations`) composed into the `Module` supertrait
  before the second domain lands.
- **Risk**: argon2 hashing is slow on tiny VMs → *Mitigation*: keep
  parameters at OWASP minimum; document minimum 1 vCPU requirement.
- **Risk**: SQLite under concurrent writes → *Mitigation*: enable
  WAL mode + `busy_timeout`; document the throughput ceiling (~100
  writes/sec) as a v0.1 limitation.
- **Risk**: `figment` env-var naming clashes with shell conventions →
  *Mitigation*: double-underscore prefix is documented in `--help`
  output and the README.

## Migration Plan

This change creates the architecture from zero; there is no existing
behavior to migrate. After archiving:

1. Follow-on change `add-sites-management` adds sites as a new module.
2. `add-ssl-management`, `add-databases-management`, `add-files-management`
   follow the same pattern.
3. `add-monitoring` and `add-cron-scheduling` land in v0.2.

## Open Questions

- Should `openpanel-cli` ship as a sub-binary of `openpanel-agent`
  (one binary, multiple entry points) or as a separate binary? —
  *Default: separate binary; the CLI can connect to a remote API in
  multi-host mode later.*
- Should the frontend live in the same repo (monorepo) or a separate
  repo? — *Default: monorepo under `web/`, separate npm workspace.*
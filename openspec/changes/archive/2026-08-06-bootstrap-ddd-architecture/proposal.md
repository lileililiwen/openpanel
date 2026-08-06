# Bootstrap DDD Architecture

## Why

OpenPanel's current scaffold declares crate boundaries (`openpanel-domain`,
`openpanel-app`, `openpanel-api`, `openpanel-cli`, `openpanel-agent`) but has
no documented contracts between them, no module registry, no config layering,
and no domain code. Without an architectural baseline, every new bounded
context (sites, ssl, databases, files, monitoring, cron, audit) will leak
into a single god-file and the codebase will not scale to the MVP. We need
a DDD-style layered architecture with explicit extension points established
before any feature code lands.

## What Changes

- Define the DDD layer boundaries and dependency rules between `openpanel-domain`,
  `openpanel-app`, `openpanel-core`, `openpanel-api`, `openpanel-cli`, and
  `openpanel-agent`.
- Introduce a `Module` trait registry in `openpanel-core` so each bounded
  context registers its domain services, repositories, HTTP routes, CLI
  commands, and migrations independently.
- Define a config layering scheme: built-in defaults → `openpanel.toml` →
  environment variables → CLI overrides.
- Establish error handling conventions (`thiserror` per layer, `anyhow` only
  at composition roots).
- Implement the **identity** bounded context as the first feature: argon2
  password hashing, session tokens, RBAC, audit logging of auth events.
  Identity is the prerequisite for every other bounded context.
- Wire HTTP routes, CLI commands, and the database pool so the application
  actually starts and accepts a login.

## Capabilities

### New Capabilities

- `architecture`: DDD layering rules, module/plugin trait contract, config
  layering, error conventions, dependency direction.
- `identity`: Local users, password hashing, session lifecycle, role-based
  authorization, audit logging of authentication events.

### Modified Capabilities

_None — this is the first change; `openspec/specs/` is empty._

## Impact

- `crates/openpanel-core` — new crate: `Module` trait, `Config`, `Context`,
  error types, `tracing` init, SQLite pool factory, migration runner.
- `crates/openpanel-domain` — populate `common/` (shared value objects),
  `identity/` (User, Role, Session aggregates, domain errors).
- `crates/openpanel-app` — populate `identity/` (services, repository
  ports + SQLite adapter), `migrations/` (SQL files).
- `crates/openpanel-api` — populate `routes/`, `middleware/`, `dto/`
  (auth routes, session middleware, JSON DTOs).
- `crates/openpanel-cli` — populate `commands/` (`openpanel user`,
  `openpanel serve`, `openpanel migrate`).
- `crates/openpanel-agent` — populate with a tokio supervisor that owns
  the SQLite pool, runs background tasks (job runner stub for later).
- `Cargo.toml` — add `openpanel-core` to workspace members and to other
  crates' dependencies.
- `web/` — emit the JSON contract docs that the React frontend will
  consume (no React code yet; that's a separate change).
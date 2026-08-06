# architecture Specification

## Purpose
TBD - created by archiving change bootstrap-ddd-architecture. Update Purpose after archive.
## Requirements
### Requirement: DDD Layered Architecture

The system SHALL organize code into four layers with strict dependency
direction. Inner layers MUST NOT depend on outer layers. The layers are:

1. **Domain** (`openpanel-domain`) — entities, value objects, aggregates,
   domain events, repository **traits** (no implementations), domain errors.
   Zero I/O, zero async runtime, no `sqlx`, no `axum`, no `tokio`.
2. **Application** (`openpanel-app`) — application services (use cases),
   repository implementations (SQLite adapters live here), transaction
   scripts. Depends only on `openpanel-domain` and `openpanel-core`.
3. **Adapters / Infrastructure** — HTTP (`openpanel-api`), CLI
   (`openpanel-cli`), on-host agent (`openpanel-agent`). Each depends on
  `openpanel-app` and `openpanel-core`. Adapters translate transport
   concerns (DTOs, status codes, exit codes) into application service calls.
4. **Composition root** — the binary `main.rs` (inside the adapter crate)
   that wires modules, builds the config, opens the DB pool, and starts
   the server.

#### Scenario: Adding a new bounded context

- **WHEN** a developer wants to add a new domain (e.g. `monitoring`)
- **THEN** they create a new module folder under `openpanel-domain/monitoring/`,
  a sibling folder under `openpanel-app/monitoring/`, and implement the
  `Module` trait once. They MUST NOT touch files in other bounded contexts
  or the composition root beyond registration.

#### Scenario: Domain crate stays I/O-free

- **WHEN** `cargo build -p openpanel-domain` is run
- **THEN** it MUST compile without `sqlx`, `axum`, `tokio`, or `reqwest`
  in its dependency tree. Any such dependency is a layering violation.

### Requirement: Module Registry via Trait

The system SHALL provide a `Module` trait in `openpanel-core` that every
bounded context implements. The trait MUST expose:

- `name() -> &'static str` — kebab-case identifier used in config, logs,
  and URLs.
- `config_schema() -> serde_json::Value` — JSON Schema fragment for the
  module's config section, merged at startup.
- `migrations() -> Vec<Migration>` — list of SQL migrations owned by the
  module, run in lexical order.
- `routes() -> Vec<axum::Router>` — HTTP routes mounted under
  `/api/v1/{name}/...`.
- `cli_commands() -> Vec<clap::Command>` — CLI subcommands under
  `openpanel {name} ...`.
- `background_tasks() -> Vec<Box<dyn BackgroundTask>>` — long-running
  tokio tasks spawned by the agent.

Registration at startup MUST be a single function call
(`app.register(IdentityModule)`) and adding a module MUST NOT require
editing any unrelated file.

#### Scenario: Registering a new module

- **WHEN** `bootstrap-ddd-architecture` is applied and `monitoring` is
  added later
- **THEN** the developer adds `app.register(MonitoringModule)` in the
  composition root and the routes, CLI, migrations, and config section
  for monitoring appear automatically.

### Requirement: Config Layering

The system SHALL load configuration from four sources in increasing
priority, later sources overriding earlier ones:

1. Built-in defaults compiled into the binary (`config/default.toml`).
2. System config file at `/etc/openpanel/openpanel.toml` (if readable).
3. User config file at `$OPENPANEL_CONFIG` or `./openpanel.toml`.
4. Environment variables prefixed with `OPENPANEL__` (double underscore
   separates path levels, e.g. `OPENPANEL__SERVER__PORT=8443`).
5. CLI flags (highest priority).

Each `Module` declares its own config schema and the merged schema MUST
be validated with `jsonschema` at startup; invalid config MUST abort with
a non-zero exit code and a human-readable error pointing at the failing
path.

#### Scenario: Environment overrides file

- **WHEN** `/etc/openpanel/openpanel.toml` sets `server.port = 8080` and
  `OPENPANEL__SERVER__PORT=8443` is set
- **THEN** the running process binds port 8443.

#### Scenario: Invalid config rejected

- **WHEN** the merged config fails JSON Schema validation
- **THEN** startup aborts with exit code 78 (`EX_CONFIG`) and the error
  message lists the failing path and a suggested fix.

### Requirement: Error Handling Conventions

The system SHALL use `thiserror` for typed errors in domain and
application layers and `anyhow::Result` only at composition roots
(adapter `main.rs`, CLI entry points). HTTP adapters MUST map domain
errors to status codes via a single `IntoResponse` impl per error type.
Sensitive error details (passwords, tokens, stack traces in production
builds) MUST NOT be leaked to API clients.

#### Scenario: Domain error in HTTP response

- **WHEN** any application service returns a typed domain error
- **THEN** the API responds with the mapped status code and body
  `{"error": "<snake_case_code>"}` and no internal detail.

### Requirement: Database Per-Module Migrations

The system SHALL store migrations in `openpanel-app/src/migrations/<module>/`
as forward-only SQL files named `V001__init.sql`, `V002__add_index.sql`,
etc. A migration runner MUST apply pending migrations in lexical order
at startup, recording applied versions in a `_migrations` table. Modules
MUST own their schema and MUST NOT alter another module's tables.

#### Scenario: First boot

- **WHEN** `openpanel serve` starts against an empty database
- **THEN** all registered modules' migrations run in order, the
  `_migrations` table is populated, and the server begins accepting
  requests.

### Requirement: Audit Trail of Cross-Cutting Events

The system SHALL provide an `audit` schema in the SQLite database
recording security-relevant events. The first audit-emitting context is
identity (auth events); future contexts (sites, databases, files) SHALL
emit audit events when they mutate state. The audit table SHALL be
append-only.

#### Scenario: Module writes an audit event

- **WHEN** any application service calls `audit.record(event)` with a
  timestamp, actor, action, target, and result
- **THEN** a row is appended to `audit_log` and the row CANNOT be
  updated or deleted by the application's own database user.

### Requirement: Extension Points for Future Domains

The architecture SHALL leave explicit extension points for domains that
will land after MVP:

- **Monitoring** — `openpanel-domain/monitoring/` and
  `openpanel-app/monitoring/` directories already exist as placeholders;
  adding them later requires no schema migration to other tables.
- **Cron / scheduling** — a `Job` trait in `openpanel-core` consumed by
  the agent's background task loop.
- **Plugins** — out-of-process WASM is a non-goal for v0.1; the in-process
  `Module` trait is the only extension surface.

#### Scenario: Adding monitoring later

- **WHEN** the `add-monitoring` change is proposed after MVP
- **THEN** it adds `crates/openpanel-domain/monitoring/`,
  `crates/openpanel-app/monitoring/` with migrations, and a single
  registration call — no edits to identity, sites, ssl, databases, or
  files code paths.


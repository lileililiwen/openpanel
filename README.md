# OpenPanel

A memory-safe, Rust-based, open-source server management panel — a Baota /
cPanel alternative without PHP. MIT licensed.

> Status: **v0.1-alpha**. The first bounded context (identity + auth) is
> working end-to-end. Sites / SSL / databases / files land in follow-on
> OpenSpec changes against this architectural baseline.

## Why Rust

Baota, cPanel, DirectAdmin, and similar panels are written in PHP. PHP
panels have a long history of RCEs, file-upload vulns, and supply-chain
incidents (composer dependency hijacks, malicious plugins). OpenPanel is a
single static binary with **no script engine at runtime** — no eval, no
file inclusion, no `system()` from user input. Everything is enforced at
compile time by Rust's type system and at runtime by axum middleware.

## Layered architecture (DDD)

```
            ┌──────────────────────────────────────────────┐
            │ openpanel-cli   /   openpanel-agent (binary)│
            │   HTTP  ◀─────▶  axum + tower middleware    │
            └────────────┬─────────────────────────────────┘
                         │
            ┌────────────▼─────────────────────────────────┐
            │ openpanel-api         (HTTP adapter)        │
            │ openpanel-cli         (CLI adapter)         │
            └────────────┬─────────────────────────────────┘
                         │
            ┌────────────▼─────────────────────────────────┐
            │ openpanel-app        (use cases + adapters) │
            │   IdentityService, SqliteUserRepository     │
            └────────────┬─────────────────────────────────┘
                         │
            ┌────────────▼─────────────────────────────────┐
            │ openpanel-domain     (entities, VOs, traits)│
            │   User, Session, Role, Email, Password      │
            └──────────────────────────────────────────────┘
                         ▲
            ┌────────────┴─────────────────────────────────┐
            │ openpanel-core        (cross-cutting)       │
            │   Module trait, Config, DatabaseDriver,     │
            │   AuditService, JobSupervisor, tracing      │
            └──────────────────────────────────────────────┘
```

Layer boundaries are enforced as separate crates — `cargo` refuses to
build if `openpanel-domain` accidentally imports `sqlx`. Adding a new
bounded context (sites, ssl, databases, files, monitoring, cron) is a
single `register()` call on the `ModuleRegistry`.

## Spec-first development

This project uses [OpenSpec](https://github.com/Fission-AI/OpenSpec).
Specs live in `openspec/`:

- `openspec/specs/<capability>/spec.md` — source of truth for a capability
- `openspec/changes/<change>/` — proposed change with proposal, design,
  specs delta, and task list

Active change:

- `bootstrap-ddd-architecture` — establishes the layered architecture and
  the **identity** capability. Archived once v0.1 ships.

## Quickstart

```bash
# Build
cargo build --release

# Run migrations + start the API server on :8080
./target/release/openpanel serve

# Create an owner (in another shell)
OPENPANEL__DATABASE__URL=/var/lib/openpanel/openpanel.db \
  ./target/release/openpanel user create \
  --username admin --email admin@example.com \
  --password 'a strong password (≥12 chars)' \
  --role owner

# Log in
curl -X POST http://127.0.0.1:8080/api/v1/identity/login \
  -H 'Content-Type: application/json' \
  -d '{"username_or_email":"admin","password":"a strong password (≥12 chars)"}'
```

## Configuration layering

Lower-numbered sources are overridden by higher-numbered ones:

1. Built-in defaults compiled into the binary
2. `/etc/openpanel/openpanel.toml`
3. `$OPENPANEL_CONFIG` or `./openpanel.toml`
4. `OPENPANEL__*` environment variables (`__` separates path levels)
5. CLI flags

The merged config is validated against a JSON Schema at startup; bad
config aborts with exit code 78 (`EX_CONFIG`) and a pointer to the
failing path.

## What works in v0.1-alpha

- **Identity / auth** — argon2id password hashing (OWASP 2024
  parameters), server-side sessions with opaque tokens (256-bit, hashed
  in DB), RBAC (Owner / Admin / User), append-only audit log.
- **Sites / nginx vhost provisioning** — `Site` aggregate with per-site
  RBAC, automatic `/etc/nginx/conf.d/openpanel/<domain>.conf` generation,
  `nginx -t && nginx -s reload` with rollback, document root
  provisioning under `/var/www/<domain>/public_html`.
- **Databases / MySQL provisioning** — `Database` aggregate with
  auto-prefixed names (`{owner}_{suffix}`), AES-256-GCM password
  encryption at rest (master key from `OPENPANEL__DATABASE__MASTER_KEY`),
  MySQL CLI shell-out for `CREATE DATABASE` / `CREATE USER` / `GRANT` /
  `DROP`, per-user RBAC, password rotation returns plaintext once.
- **HTTP API** — `/api/v1/identity/*`, `/api/v1/sites/*`,
  `/api/v1/databases/*`, `/health`. Bearer + cookie auth.
- **CLI** — `openpanel serve`, `openpanel migrate`,
  `openpanel user {create,list,disable,delete}`,
  `openpanel site {create,list,delete,enable,disable}`,
  `openpanel database {create,list,delete,change-password}`.

## What's coming next (each as its own OpenSpec change)

- `add-ssl-management` — Let's Encrypt via ACME
- `add-files-management` — chrooted file manager
- `add-monitoring` — CPU / RAM / disk / network metrics
- `add-cron-scheduling` — recurring job runner

Each change adds a new bounded context, registers it on the
`ModuleRegistry`, and ships its own migration set.

## Repository layout

```
crates/
├── openpanel-core/       cross-cutting primitives (Module, Config, ...)
├── openpanel-domain/     zero-I/O entities + value objects + repo traits
├── openpanel-app/        use-case services + SQLite repository adapters
├── openpanel-api/        axum routes, middleware, DTOs
├── openpanel-cli/        clap commands + serve/migrate/user handlers
└── openpanel-agent/      standalone binary (same handler set as cli::serve)
web/                      React + Vite + TanStack Router frontend (skeleton)
openspec/                 OpenSpec specs and change proposals
```

## License

MIT — see `LICENSE`.
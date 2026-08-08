# OpenPanel

A memory-safe, Rust-based, open-source server management panel — a Baota /
cPanel alternative without PHP. MIT licensed.

> Status: **v0.1-alpha**. Identity + auth, sites, databases, files,
> SSL, and monitoring ship as bounded contexts against this
> architectural baseline. A pure-Rust HTMX web UI (login, shell,
> logout) serves as the panel front-end. Cron lands in a follow-on
> OpenSpec change.

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
single `register()` call on the `ModuleRegistry`.## Spec-first development

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

## Web UI

A pure-Rust, server-rendered control panel lives in the `openpanel-web`
crate. It uses **HTMX** for interactivity — no JavaScript build step, no
Node toolchain. The shell (sidebar + topbar + content region), login
page, and logout are server-rendered `maud` templates; the same
`openpanel_session` cookie and session middleware back both the API and
the web UI, so there is a single auth system.

- **Login** — `GET /login` renders the form; `POST /login` authenticates
  via `IdentityService`, sets the `openpanel_session` cookie
  (HttpOnly, `SameSite=Lax`, `Max-Age=86400`), and redirects to `/`.
- **Shell** — `GET /` renders the HTMX shell: sidebar nav (with
  `hx-boost`), topbar with the logged-in user, and a content region.
- **Logout** — `POST /logout` invalidates the session and clears the
  cookie. State-changing web POSTs require a per-session CSRF token
  (`_csrf`); mismatches return `403`.
- **Assets** — `htmx.min.js` (pinned, with license header) and
  `app.css` are embedded via `include_bytes!` and served under
  `/assets/*`.

```bash
# Boot the panel (API + web UI on :8080)
cargo run --release -- serve

# Open a browser
xdg-open http://127.0.0.1:8080/login
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
- **Files / chrooted file manager** — list / read / write / mkdir /
  rename / chmod / remove per site; canonicalize-once-per-request
  chroot check (no path traversal); multipart upload; 50 MB read cap;
  RBAC mirroring the sites module.
- **Monitoring / host resource metrics** — pure-Rust collection via
  `sysinfo` (CPU / RAM / disk / network), append-only SQLite time
  series with a rolling retention window (default 7 days),
  config-driven alert thresholds with hysteresis (audit-log events in
  v0.1), and a background collector task.
- **HTTP API** — `/api/v1/identity/*`, `/api/v1/sites/*`,
  `/api/v1/databases/*`, `/api/v1/files/*`, `/api/v1/monitoring/*`,
  `/health`. Bearer + cookie auth.
- **CLI** — `openpanel serve`, `openpanel migrate`,
  `openpanel user {create,list,disable,delete}`,
  `openpanel site {create,list,delete,enable,disable}`,
  `openpanel database {create,list,delete,change-password}`,
  `openpanel file {list,read,write,mkdir,rm,rename,chmod}`,
  `openpanel monitoring {overview,history}`.

## What's coming next (each as its own OpenSpec change)

- `add-cron-scheduling` — recurring job runner

Each change adds a new bounded context, registers it on the
`ModuleRegistry`, and ships its own migration set.

## Testing

Four test categories live in this repo:

| Category | Where | Run with |
|---|---|---|
| Unit | `#[cfg(test)]` in each module | `cargo test --workspace` |
| Property | `proptest!` in `openpanel-domain` | `cargo test --workspace` |
| Integration | `tests/integration/*.rs` | `cargo test -p openpanel --test integration` |
| CLI E2E | `tests/cli/*.rs` | `cargo test -p openpanel --test cli_*` |

Integration tests boot a real axum router against a per-test SQLite DB
and a sandboxed temp directory for nginx configs / document roots.
See `tests/README.md` and `crates/openpanel-test-support/README.md`.

```bash
# Run everything
./scripts/check-tests.sh

# A single failing test
cargo test -p openpanel --test integration sites::sites_list_empty_for_fresh_db -- --nocapture

# More proptest cases for CI
PROPTEST_CASES=1000 cargo test --workspace
```

## Quality

The codebase enforces a strict quality policy via `clippy.toml` and the
root `Cargo.toml` `[workspace.lints]` block. The intent: catch bugs at
compile time rather than in production.

**Rules:**

- `unsafe_code = "forbid"` — no `unsafe` blocks anywhere.
- `unwrap_used = "deny"` / `expect_used = "deny"` / `panic_used = "deny"` /
  `todo = "deny"` / `unimplemented = "deny"` — in **production code**.
  Test code is exempt.
- `missing_docs = "warn"` — every public item should have a doc comment.
- `rustdoc::broken_intra_doc_links = "deny"` — enforced per-crate.
- `disallowed-methods = ["std::panic::catch_unwind"]` in `clippy.toml`.

**Local checks:**

```bash
make check                 # fmt + clippy + doc + audit + test
make fmt                   # or run a single gate: make clippy, ...
```

**CI:** `.github/workflows/ci.yml` runs the same gates on every push.

**Dependency audit:** `cargo audit` is part of the quality gate. Known
false positives go in `.cargo/audit.toml`.

## SSL / TLS

Every managed site is automatically wired to HTTPS once an SSL
certificate is installed. The panel handles the full lifecycle:

- **Let's Encrypt HTTP-01** — one-click issuance against
  `https://acme-staging-v02.api.letsencrypt.org/directory` by default
  (production opt-in via `--production`). The local ACME HTTP-01
  challenge server binds to `127.0.0.1:9080` and nginx's port-80
  vhost proxies `/.well-known/acme-challenge/` to it.
- **Manual PEM upload** — paste cert + chain + private key for a
  domain; the key is stored encrypted at rest.
- **Self-signed generation** — `rcgen`-backed, for dev / internal
  services.
- **Auto-renewal** — a daily background task re-issues ACME certs
  whose `valid_to - now < 30 days`.
- **Force-HTTPS 301** — per-site toggle; default on when a cert is
  active. The nginx render uses `location ^~` priority so ACME
  renewals still work even with force-HTTPS on.

The default ACME endpoint is **staging** so fresh installs don't
burn Let's Encrypt rate limits or produce real public certs. Switch
to production explicitly via the API or CLI.

**CLI:**

```bash
openpanel ssl list
openpanel ssl issue example.com --production
openpanel ssl upload example.com --cert cert.pem --key key.pem [--chain chain.pem]
openpanel ssl self-signed internal.example.com
openpanel ssl revoke example.com
openpanel ssl renew example.com
```

**API** (under `/api/v1/ssl/*`):

```
GET    /certificates                              list
GET    /certificates/{domain}                     fetch one
POST   /certificates/acme                         ACME HTTP-01 issue
POST   /certificates/manual                       upload PEM
POST   /certificates/self-signed                  self-signed
DELETE /certificates/{domain}                     revoke + delete
POST   /certificates/{domain}/renew               force-renew
PATCH  /certificates/{domain}/force-https         toggle 301
```

Private-key material is **never** returned in any response — only
metadata. See `crates/openpanel-app/src/ssl/README.md` for the full
public surface, renewal policy, and storage envelope.

## Monitoring

Host resource monitoring — CPU / RAM / disk / network — collected
entirely in-process via `sysinfo` (no `sar`/`vmstat`/`top` shell-outs):

- **Collection** — a background task snapshots global CPU and memory
  utilization, per-mount disk usage, and aggregate network throughput
  every `interval_secs` (default 60).
- **Time series** — samples are appended to a SQLite table keyed by
  `(ts, kind)` and pruned after `retention_days` (default 7; `0`
  disables pruning).
- **Alerts** — optional `[monitoring] alert.*` thresholds
  (`cpu_percent`, `memory_percent`, `disk_percent`). A rule fires
  exactly once per crossing (hysteresis) and writes an
  `AlertFired` audit event. v0.1 has no delivery channels yet.
- **Dashboard data** — `/api/v1/monitoring/overview` always collects a
  fresh snapshot (real host values); `/history` is a range scan.

**CLI:**

```bash
openpanel monitoring overview                 # current host snapshot
openpanel monitoring history --metric Cpu     # recent samples (last 3600s)
```

**API** (under `/api/v1/monitoring/*`):

```
GET /overview                current host snapshot (fresh collection)
GET /history?metric=Cpu&range=3600   time series for one metric kind
GET /alerts?limit=20         recent AlertFired audit events
```

See `crates/openpanel-app/src/monitoring/README.md` for the collection
model, storage shape, and alerting rules.

## Repository layout

```
crates/
├── openpanel-core/       cross-cutting primitives (Module, Config, ...)
├── openpanel-domain/     zero-I/O entities + value objects + repo traits
├── openpanel-app/        use-case services + SQLite repository adapters
├── openpanel-api/        axum routes, middleware, DTOs
├── openpanel-cli/        clap commands + serve/migrate/user handlers
├── openpanel-agent/      standalone binary (same handler set as cli::serve)
└── openpanel-test-support/  TestDb, TestServer, mocks (dev-only)
web/                      React + Vite + TanStack Router frontend (skeleton)
openspec/                 OpenSpec specs and change proposals
scripts/                  check-fmt/clippy/docs/audit/tests.sh, coverage.sh
Makefile                  quality gate entry point (`make check`)
tests/                    integration + CLI E2E tests
.github/workflows/        CI pipelines
```

## License

MIT — see `LICENSE`.
# OpenPanel

A memory-safe, Rust-based, open-source server management panel — a Baota /
cPanel alternative without PHP. Single static binary, no script engine at
runtime. MIT licensed.

> Status: **v0.x in active development**. Identity + auth, sites, databases,
> files, SSL, mail, DNS, monitoring, cron, software center, an
> owner-only audit activity center, an mTLS agent fleet, and a pure-Rust
> HTMX web UI ship as bounded contexts against this architectural
> baseline. OpenSpec has 83 live capabilities and 124 archived changes.
> The `make check` quality gate is green end-to-end on `main`.

## Quick start (dev)

```bash
cargo run -p openpanel-cli -- dev
```

Picks `/tmp/openpanel-dev` as the data dir (override with
`OPENPANEL_DATA_DIR`), auto-generates a master key, runs migrations,
bootstraps a default `admin` owner (password `openpanel-dev` —
**change it**), and serves the web + API on
`http://127.0.0.1:8080`. Open `http://127.0.0.1:8080/login` and
sign in. Idempotent — re-running picks up the existing DB + key.

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
bounded context is a single `register()` call on the `ModuleRegistry`.
The crates are:

| Crate | Role |
|---|---|
| `openpanel-core` | cross-cutting primitives (Module trait, Config, DatabaseDriver, AuditService, JobSupervisor, tracing) |
| `openpanel-domain` | entities, VOs, aggregates, repository traits — **zero I/O** |
| `openpanel-app` | use-case services, SQLite repository impls, migrations, modules |
| `openpanel-api` | axum routes, middleware, DTOs |
| `openpanel-cli` | clap commands + serve/migrate/user handlers |
| `openpanel-agent` | standalone mTLS agent that enrolls a host into the fleet |
| `openpanel-web` | server-rendered HTMX web UI (maud templates) |
| `openpanel-test-support` | `TestDb`, `TestServer`, mocks (dev-only) |

## Spec-first development

This project uses [OpenSpec](https://github.com/Fission-AI/OpenSpec).
Specs live in `openspec/`:

- `openspec/specs/<capability>/spec.md` — source of truth for a capability
- `openspec/changes/<name>/` — proposed change with proposal, design,
  specs delta, and task list
- `openspec/changes/archive/<date>-<name>/` — frozen history of every
  shipped change (currently 124 archives)
- `openspec/governance/manifest.yaml` — ratchet pinning the four
  governance capabilities (`agent-quality`, `quality`, `testing`,
  `architecture`); `make governance-contract` enforces text + scenario
  count + executable checker for every protected requirement

Workflow: `propose → validate → implement (apply) → archive`.
`make check` is the single quality-gate entry point and runs:
`fmt → clippy → docs → audit → file-length → scan-literal →
tasks-testing-first → reuse → layering → spec-test-drift → spec-drift →
test-gates → test`. `make test-gates` runs the governance self-test
harness in isolation (16 fixture-based self-tests, positive + negative
for every gate). `make agent-governance` re-verifies the OpenSpec
context and every runtime contract link. See `HANDOFF.md` for the
current roadmap and the latest spec status.

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
- **Operations dashboard** — role-aware widgets, server-identity
  header, host gauges (CPU/RAM/disk/network), per-mount and
  per-interface metrics, an attention queue, role-scoped quick actions,
  and a CPU trend sparkline.
- **Site workspace** — tabbed navigation (Overview, Files, HTTP
  controls, WAF, Staging, Cache+CDN, Collaborators, FTP) with a
  capability-filtered tab list and site-bar chrome preserved across
  mutations.
- **Audit activity center** — owner-only `/audit` page with
  redaction-aware event list, cursor pagination, and CSV export.
- **Software Center** — production storefront at `/software` with
  category tabs, search, filters, detail page (Overview/Versions/
  Changelog/Dependencies/Source), install wizard, and job progress.

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

## What ships today (v0.x in active development)

Bounded contexts that are implemented and shipping in `main`. The full
capability surface is described in `openspec/specs/`:

- **Identity / auth** — argon2id password hashing (OWASP 2024
  parameters), server-side sessions with opaque tokens (256-bit, hashed
  in DB), RBAC (Owner / Admin / User), SSO (OIDC) with active-session
  inventory and revoke, TOTP 2FA with recovery codes, append-only
  audit log, redacted by default.
- **Sites / nginx vhost provisioning** — `Site` aggregate with
  per-site RBAC, automatic `/etc/nginx/conf.d/openpanel/<domain>.conf`
  generation, `nginx -t && nginx -s reload` with rollback, document
  root provisioning under `/var/www/<domain>/public_html`,
  per-site HTTP controls (custom error pages, redirect rules,
  protected dirs, hotlink protection, MIME overrides,
  directory-index policy, IP allow/deny), per-site PHP runtime,
  transport tuning, site staging, site clone + templates,
  preview deployments, cache + CDN, WAF rules.
- **Databases / MySQL provisioning** — `Database` aggregate with
  auto-prefixed names (`{owner}_{suffix}`), AES-256-GCM password
  encryption at rest, MySQL CLI shell-out for `CREATE DATABASE` /
  `CREATE USER` / `GRANT` / `DROP`, per-user RBAC, remote-access
  enforcement, password rotation returns plaintext once, PITR
  (point-in-time restore).
- **Files / chrooted file manager** — list / read / write / mkdir /
  rename / chmod / remove per site; canonicalize-once-per-request
  chroot check (no path traversal); multipart upload; 50 MB read cap;
  RBAC mirroring the sites module.
- **FTP / SFTP / jailed shells** — per-account FTP with the sites
  chroot, SFTP-only jailed shells.
- **SSL / TLS** — Let's Encrypt HTTP-01 (staging default, production
  opt-in), DNS-01 wildcard, manual PEM upload, self-signed
  generation, auto-renewal, force-HTTPS 301, per-site toggle. Private
  keys never leave the panel; ACME HTTP-01 challenge server is
  local-only (`127.0.0.1:9080`).
- **Mail** — hosted mail domains, mailboxes, aliases, catch-alls,
  per-mailbox autoresponder + quota, mailing lists, DKIM auto-generation
  + rotation with grace, greylisting, spam scoring + spam-folder
  routing, Sieve filter management, deliverability monitoring, safe
  MTA/IMAP configuration.
- **DNS** — zone management, default zone templates, template
  application lifecycle with preview + re-apply, per-owner template
  overrides, typed record lifecycle, zone synchronization, DNS
  automation, DNS provider accounts.
- **Monitoring / host resource metrics** — pure-Rust collection via
  `sysinfo` (CPU / RAM / disk / network), append-only SQLite time
  series with a rolling retention window (default 7 days),
  config-driven alert thresholds with hysteresis, bandwidth
  accounting, quotas (bandwidth, disk, inode). Synthetic monitoring
  with a public status page (`add-status-page`).
- **Backups** — backup plans, scheduled runs, retention, integrity
  checks, secret safety, offsite targets, restore drills, typed
  restore scope, per-plan schedule constraints, PITR.
- **Cron** — scheduled jobs, due-job execution, execution history
  retention, per-user cron quotas, job scope and role permissions.
- **Software Center** — Owner-only trusted catalog, host discovery,
  reviewable Nginx/PHP/MySQL/MariaDB/Redis lifecycle plans, durable
  jobs, safe cancellation/retry/rollback, pinned WordPress/Drupal
  deployment recipes, digest state (Verified/Placeholder/Missing/
  Invalid/NotApplicable), 30+ production-seed entries (Nginx, Apache,
  OpenLiteSpeed, PHP 7.4–8.3, MySQL 5.7/8.0, MariaDB, PostgreSQL 16,
  Redis, Memcached, WordPress, Drupal, Joomla, Ghost, Nextcloud,
  Matomo, Gitea, Mattermost, …). See [the operator guide](docs/software-center.md).
- **Web application installer** — WordPress toolkit, generic
  artifact placement, application deploy commands.
- **Container runtime + registry** — Docker-compatible container
  management, container registry.
- **Agent fleet** — mTLS-enrolled `openpanel-agent` binary on each
  host, signed recipe execution, fleet aggregation, agent read-only
  API, fleet surfaces.
- **Security** — host firewall (nftables, owned `inet openpanel`
  table only, with recovery procedure), admin IP allowlist, SSH key
  lifecycle, host security hardening.
- **Notifications** — channels + subscriptions, escalation, delivery
  to in-panel and external targets.
- **API tokens** — scoped bearer tokens with rotation + revoke.
- **Quotas** — bandwidth / disk / inode enforcement per owner and
  per site; quota sampler integrates with monitoring.
- **Collaboration** — collaborators (per-site), account hierarchy
  (Owner → Admin → User), hosting plans, billing export.
- **i18n / theming** — translation pipeline, brandable UI tokens
  (`tokens.css`), themable UI surface.
- **Migration importers** — cPanel / Plesk / DirectAdmin import paths.
- **IaC** — Terraform SDK; programmatic panel configuration.
- **AI-ops** — bounded LLM integration (read-only diagnostics,
  explain-my-config).
- **Audit activity** — owner-only `AuditService::query` with
  redaction allowlist + cursor pagination; UI at `/audit` and JSON
  API at `/audit/events`.
- **Operations dashboard + workflow models** — role-aware widgets,
  file/DB/backup decision models (confirmation, recoverable flag,
  capacity evaluation, secret-safe `DatabaseRowView`).
- **Site workspace + terminal** — tabbed site context, terminal +
  host-fleet decision models (command safety classification, session
  expiry, host view redacting cert/key material).
- **Compliance** — audit-log completeness, retention, tamper-evidence.
- **Feedback widget + UI state vocabulary** — user feedback
  channel + a global `<EmptyState>` / `<ErrorState>` / `<TaskState>`
  vocabulary used across all routes.
- **HTTP API** — 40+ route modules under `/api/v1/*` (identity,
  sites, databases, files, monitoring, mail, dns, cron, backups,
  software, security, …). Bearer + cookie auth.
- **CLI** — `openpanel` with 60+ subcommands (see below).
- **Web UI** — `openpanel-web` crate with the full panel shell:
  login, shell, sites, databases, files, mail, DNS, software,
  audit, monitoring, status page, terminal, settings.

## What's coming next (each as its own OpenSpec change)

See `HANDOFF.md` and `openspec/changes/` for the live roadmap. The
`docs/competitive-gap-analysis.md` lists the remaining items 8–10
(object-storage hosting, CalDAV/CardDAV/WebDAV, mailing-list
moderation depth) that have no OpenSpec change folder yet.

## Testing

Four test categories live in this repo:

| Category | Where | Run with |
|---|---|---|
| Unit | `#[cfg(test)]` in each module | `cargo test --workspace` |
| Property | `proptest!` in `openpanel-domain` | `cargo test --workspace` |
| Integration | `tests/integration/*.rs` (includes the `web_ui_styling` form + responsive contract suite) | `cargo test -p openpanel --test integration` |
| CLI E2E | `tests/cli/*.rs` | `cargo test -p openpanel --test cli_*` |

Integration tests boot a real axum router against a per-test SQLite DB
and a sandboxed temp directory for nginx configs / document roots. The
`web_ui_styling` integration suite additionally walks every public
form on every public route to enforce the global form-class vocabulary,
the paired-label rule, and the no-ad-hoc-tokens contract. See
`tests/README.md` and `crates/openpanel-test-support/README.md`.

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
make check                 # full gate: fmt, clippy, docs, audit, file-length, scan-literal, tasks-testing-first, reuse, layering, spec-test-drift, spec-drift, test-gates, test
make test-gates            # the governance self-tests in isolation (16 fixture-based checks)
make agent-governance      # OpenSpec context + runtime contract integrity
make governance-contract   # archived governance content ratchet
make fmt                   # or run a single gate: make clippy, ...
```

**CI:** `.github/workflows/ci.yml` runs `make check` on every push
through the `check` job. The `agent-quality` job additionally runs
`make test-gates` and `openspec validate --all --strict
--no-interactive`.

**Dependency audit:** `cargo audit` is part of the quality gate. Known
false positives go in `.cargo/audit.toml`.

## CLI surface (current `main`)

```text
openpanel serve                              start the API + web UI
openpanel migrate                            run SQLite migrations
openpanel dev                                dev-mode bootstrap (data dir, master key, owner, server)

# Identity / auth
openpanel user {create,list,disable,delete}
openpanel auth login / logout / whoami
openpanel two-factor {enable,disable,verify}
openpanel recovery-code generate

# Sites
openpanel site {create,list,delete,enable,disable,clone,template,...}
openpanel site http {error-pages,redirects,protected-dirs,hotlink,...}
openpanel site transport {tune,reset,show}
openpanel site staging {start,promote,destroy,...}
openpanel site cache {status,purge,...}
openpanel cdn {add,remove,purge,...}
openpanel waf {rule,list,enable,disable}
openpanel site-preview {start,stop,list,show}
openpanel collaborator {add,remove,list,role}

# Databases
openpanel database {create,list,delete,change-password}
openpanel db remote-access {grant,revoke,show}
openpanel pitr {list,restore,show}

# Files
openpanel file {list,read,write,mkdir,rm,rename,chmod}

# SSL
openpanel ssl {list,issue,upload,self-signed,revoke,renew}

# Mail
openpanel mail {domain,mailbox,alias,list,sieve,autoresponder,quota,dkim,...}

# DNS
openpanel dns {zone,record,template,...}

# Monitoring + status page
openpanel monitoring {overview,history,alerts}
openpanel status-page {show,publish,unpublish,...}

# Software center
openpanel software {catalog,inventory,search,show,preview,install,adopt,update,uninstall,deploy,jobs,cancel,retry,rollback,diagnostics,refresh}

# Backups
openpanel backup {plan,run,list,show,restore,drill,...}

# Cron
openpanel cron {create,list,show,delete,enable,disable,run-now,history}

# Containers
openpanel docker {ps,images,run,stop,rm,logs,...}
openpanel container-runtime {list,start,stop,...}
openpanel registry {list,push,pull,delete,...}

# Security
openpanel security {preview,apply,rollback,allowlist,rule,ssh-keys}

# Logs
openpanel logs {list,show,policy,rotate,prune}

# Notifications
openpanel notification {channel,subscription,list,test}

# API tokens
openpanel token {create,list,show,rotate,revoke}

# IaC
openpanel iac {plan,apply,destroy,show,export,import}

# Plugins + marketplace
openpanel plugin {list,install,enable,disable,show}
openpanel marketplace {list,install,show}

# Web terminal (browser-side, server-side session only)
openpanel terminal session {start,list,close}

# Audit
openpanel audit list / show / export

# Hosting plans
openpanel hosting-plan {list,create,show,delete,assign}

# Server snapshots
openpanel server-snapshot {create,list,restore,delete}

# Settings / branding / i18n
openpanel branding {show,set,reset}
openpanel i18n {list,set,export}
```

## Repository layout

```
crates/
├── openpanel-core/          cross-cutting primitives (Module, Config, AuditService, ...)
├── openpanel-domain/        zero-I/O entities + value objects + repo traits
├── openpanel-app/           use-case services + SQLite repository adapters + migrations
├── openpanel-api/           axum routes, middleware, DTOs
├── openpanel-cli/           clap commands + serve/migrate/user handlers
├── openpanel-agent/         standalone mTLS agent binary
├── openpanel-web/           server-rendered HTMX web UI (maud templates)
└── openpanel-test-support/  TestDb, TestServer, mocks (dev-only)
openspec/                    OpenSpec specs (83 capabilities) and change proposals (124 archived)
docs/                        operator guides (TODOS, software-center, firewall-recovery, competitive gap)
scripts/                     check-fmt/clippy/docs/audit/file-length/test-gates.sh, repo-map.sh, coverage.sh
tests/                       integration + CLI E2E + web-ui styling tests
.github/workflows/           CI pipeline
HANDOFF.md                   live roadmap + spec status
Agents.md                    normative agent contract (single source of truth)
AGENTS.md                    symlinked entry point for every AI agent runtime
Makefile                     single quality-gate entry point (`make check`)
```

## License

MIT — see `LICENSE`.

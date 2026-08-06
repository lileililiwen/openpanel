# Design: Add Sites Management

## Context

`bootstrap-ddd-architecture` established the layered architecture and the
identity context. This change adds the first feature on top: nginx vhost
provisioning. The site module owns:

- The `sites` SQLite table
- The `Site` aggregate and its invariants
- nginx config generation and reload
- Per-site RBAC
- CLI and HTTP surface

The site module is registered via `SitesModule::new(&ctx)` in the
composition root and contributes migrations, routes (via
`Module::routes`), and the service handle.

## Goals / Non-Goals

**Goals:**

- Provision an nginx vhost in a single `create_site` call.
- Reload nginx safely (test before reload, roll back on failure).
- Enforce per-site RBAC at the service layer (not the HTTP layer).
- Keep the change self-contained — no edits outside `sites/` folders
  and the composition root's `register()` call.

**Non-Goals:**

- PHP-FPM pool management (toggle is persisted; pool provisioning is v0.2).
- SSL provisioning (separate `add-ssl-management` change).
- Apache / Caddy support (nginx only in v0.1).
- Webroot file upload / management (`add-files-management` is later).
- Multi-tenant isolation (sites share `/var/www/` and the openpanel
  system user; chroot is a v0.2+ hardening item).

## Decisions

### 1. nginx config path

**Decision**: `/etc/nginx/conf.d/openpanel/<domain>.conf` for active
sites, `/etc/nginx/conf.d/openpanel/disabled/<domain>.conf.disabled` for
disabled sites.

**Rationale**: `conf.d/` is the conventional include path on Debian /
Ubuntu / RHEL nginx installs. A subdirectory `openpanel/` makes the
ownership obvious and lets us selectively disable by file extension
(`.disabled`).

**Alternatives considered**:

- `/etc/nginx/sites-enabled/` — Debian convention but requires
  symlinking into `sites-available/`; more moving parts.
- Per-site file owned by the user — would require granting the openpanel
  process write access per-user; not worth the complexity for v0.1.

### 2. Config generation strategy

**Decision**: Hand-written `format!()` template in
`openpanel-app/src/sites/nginx.rs`. No templating engine.

**Rationale**: The config we generate is small (< 30 lines) and
constant. Adding `tera` or `handlebars` for one template is overkill.
`format!()` is easy to read and easy to test.

**Alternatives considered**:

- Tera templates in `sites/templates/` directory — flexible but adds a
  dependency for a single use case.

### 3. Reload mechanism

**Decision**: Shell out to `nginx -t && nginx -s reload` via `std::process::Command`.

**Rationale**: nginx has no library API. `nginx -t` validates config
without affecting the running server; `nginx -s reload` performs a
graceful reload. Both commands are universally available on Linux
installs.

**Trade-off**: This requires the openpanel process to be able to invoke
nginx. The MVP assumes the process runs as root. A later change can
add sudoers drop-in support so it runs as a dedicated user.

### 4. Per-site RBAC at the service layer

**Decision**: The `SitesService` methods accept a `&User` `caller`
parameter and enforce permissions; the HTTP layer just passes the
authenticated user through.

**Rationale**: Putting RBAC at the service layer means a CLI command
with the same caller gets the same enforcement. Symmetric.

### 5. Document root ownership

**Decision**: Files owned by the site's owner, looked up via
`/etc/passwd` (Linux only). The MVP uses the owner's username; UID
lookup happens via `users::get_user_by_name()` from the `users` crate.

**Rationale**: This is what Baota / cPanel do — site owners can `ssh`
in as themselves and edit files. We mirror that.

**Trade-off**: We add a `users` crate dep. It's a tiny crate, pure C
binding.

### 6. Migration ownership

**Decision**: `crates/openpanel-app/src/migrations/sites/V001__init.sql`.
The architecture-level migration runner discovers and applies it on
`openpanel serve`.

**Rationale**: This is exactly what `bootstrap-ddd-architecture`'s
`Module::migrations()` was designed for. Zero changes to the migration
runner.

### 7. nginx config test boundary

**Decision**: `nginx -t` MUST exit 0 before reload. If it fails, the
service restores the prior state of the conf.d directory and returns
`SiteError::NginxTest`.

**Rationale**: Failures must not leave the system with a broken config.
We snapshot the affected files in memory before write so a failed test
can restore them atomically.

## Risks / Trade-offs

- **Risk**: Running as root is required for nginx reload + chown →
  *Mitigation*: Document the requirement; add sudoers drop-in in a
  follow-up change so openpanel can run as a dedicated user.
- **Risk**: `nginx -t` race with concurrent edits → *Mitigation*: Hold
  a per-process mutex around all config writes (small surface; v0.1).
- **Risk**: Site alias enumeration bypass (e.g. `*.com`) → *Mitigation*:
  Aliases must match `^[a-z0-9.-]+$` and reject wildcards.
- **Risk**: Document root traversal (`../`) → *Mitigation*: Reject
  document_root outside `/var/www/<domain>/` unless allowlist configured.

## Migration Plan

- No existing data to migrate.
- After archive, the next composition-root change adds
  `let sites_module = SitesModule::new(&ctx).await;` plus
  `Module::migrations` registration.
- nginx install must be present on the host. The change's `serve`
  bootstrap MUST check for `nginx -V` and warn (not fail) if missing.

## Open Questions

- Should we use `nginx -s reload` or `systemctl reload nginx`? →
  *Default: `nginx -s reload`* because it works without systemd
  (containers, Alpine).
- Should PHP-FPM version auto-detect from installed PHP binaries? →
  *Default: no*, the operator passes `--php-version 8.3`. v0.1 ships the
  PHP toggle but not the pool file generation.
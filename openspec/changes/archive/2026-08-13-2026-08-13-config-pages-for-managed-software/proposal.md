# Config pages for managed software

## Why

Installs now work end-to-end from the panel UI (Baota-style): Web
artifacts and System packages install in the background with live
progress, and non-interactive privilege handling means a non-technical
owner never touches a terminal. But once a component like fail2ban,
nginx, or redis is installed there is no way to change it from the
panel. Baota ships a per-software configuration page that edits the
component's key config file (`/etc/fail2ban/jail.local`,
`/www/server/nginx/conf/nginx.conf`, ...) safely. OpenPanel currently
exposes only Update / Remove for managed components; `configuration_paths`
exists on the preview as metadata but nothing reads or writes it.

A business user who installed fail2ban to stop brute-force login
attempts cannot realistically SSH in and edit `jail.local`. This change
gives the same "click to configure" experience as the rest of the panel.

## What Changes

* **A server-owned config manifest.** A fixed allowlist table maps each
  System component to its key config file(s) and whether the component
  supports a validation pass (`nginx -t`, `php-fpm8.x -t`,
  `redis-cli ping`, `mysqladmin ping`). The browser can only read and
  write paths named by the manifest — never an arbitrary path. This is
  a deliberate security boundary: the panel is a curated config editor,
  not a root file browser.
  - `fail2ban` → `/etc/fail2ban/jail.local`
  - `nginx` → `/etc/nginx/nginx.conf`
  - `php-8.0`…`php-8.3` → `/etc/php/<version>/fpm/php.ini`
  - `mysql` → `/etc/mysql/mysql.conf.d/mysqld.cnf`
  - `mariadb` → `/etc/mysql/mariadb.conf.d/50-server.cnf`
  - `redis` → `/etc/redis/redis.conf`
* **Owner-only config endpoints.** The service gains `read_config`,
  `save_config`, and a config-manifest accessor. Both require the
  Owner role and that the component is panel-managed. Reads are bounded
  (a config file cannot be megabytes). Saves follow the panel's
  existing atomic-write discipline (write `.new`, rename, validate, and
  restore the previous content if validation fails) — the same pattern
  the nginx sites module uses for site blocks.
* **Configuration page and save route.** A new `Configuration` entry on
  the entry detail page for managed components leads to
  `GET /software/components/{id}/config` rendering the current file in
  a textarea with a Save button. `POST /software/components/{id}/config`
  persists the edit and re-renders the page with a success or an error
  banner (with the previous content restored on validation failure).
* **JSON API mirrors.** `GET/POST /api/v1/software/components/{id}/config`
  expose the same read/save operations for automation and the
  integration tests.
* **Reload is explicitly out of scope for this phase.** Restarting the
  service after an edit is already possible from the allowlisted
  Services section; auto-reload with a per-component reload command is
  a follow-up capability (tracked in the design).

## Capabilities

### Modified Capabilities

- `software-center`: the existing `System Component Planning and
  Lifecycle` capability gains the curated, owner-only config editor for
  panel-managed components.

## Requirements

CLI:

- [ ] **Owner-only contract.** $GHOST_DONE(system-install-progress:
  Owner-only) holds for config reads and saves.

UNSTABLE:

- [ ] **Config manifest is server-owned and fixed.** Requirements:
  A server-owned allowlist (the config manifest) maps component id to
  key config file paths.
  Scenario: An owner opens the configuration page for `fail2ban` and
  sees `/etc/fail2ban/jail.local` rendered.
  Scenario: A request tries to change a file path not in the manifest
  and is rejected.
- [ ] **Read reports presence and bounded content.** Requirements:
  A managed component with a manifest exposes its current config file
  content, its absolute path, and whether the file exists.
  Scenario: A managed component whose config file exists returns the
  file content bounded to a hard size ceiling.
  Scenario: A managed component whose config file does not yet exist
  returns `exists: false` with no content.
- [ ] **Save is atomic and validated.** Requirements: Saving writes the
  file atomically; when the component supports validation, a failed
  validation restores the previous content and surfaces the error.
  Scenario: A valid edit writes the new content and is readable back.
  Scenario: A component that supports validation receives a config that
  fails validation; the previous content is restored and the save is
  reported as failed.
- [ ] **Schema and stability.** Requirements: The behavior described by
  these requirements SHALL be preserved in a follow-up change that
  moves the manifest into the signed catalog or adds auto-reload.
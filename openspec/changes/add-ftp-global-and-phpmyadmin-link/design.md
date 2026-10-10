# Design: add-ftp-global-and-phpmyadmin-link

## Explore & Reuse (repo-map evidence, 2026-10-10)

`scripts/repo-map.sh` plus targeted reads confirm every
behavior needed already exists; the change adds web-adapter
glue only:

- `crates/openpanel-app` `FtpService` (`list`, `create`,
  `enable`, `disable`, `delete`, `FtpAccountView`) — the `/ftp`
  aggregate calls `list` per visible site and renders; no new
  service methods, no daemon change.
- `sftp-jailed-shells` (`SftpJailGrant`, `render_sshd_config`)
  — consulted for overlap; untouched, no shared code path.
- `crates/openpanel-app` `DatabasesService`
  (`list_databases`, `get_database`, `change_password`,
  `reveal_password`, `delete_database`) — all DB row actions
  delegate here; password plaintext stays one-time-panel only.
- Remote-access enforcement (`RemoteAccess ACL`,
  `GrantFailed` all-or-nothing, `global_access_locked`,
  `NoOpenByDefault`, boot-time reconcile) — the permission
  editor (Local / Everyone / Specified IP) maps 1:1 onto the
  existing `PUT /databases/{id}/remote-access` semantics; the
  `allow_any_source` owner opt-in is surfaced, never bypassed.
- `POST /databases/{id}/admin-tool` SSO mint (single-use
  signed token, `AdminToolLaunched` audit) — the phpMyAdmin
  deep-link and Sync-all / Get-from-server adopt flow reuse
  this mint; no vendored phpMyAdmin.
- `crates/openpanel-web/src/router.rs` — explicit mounting
  pattern; only `/sites/{id}/ftp` exists today, so the change
  adds `GET /ftp` reusing `ftp.rs` row rendering.
- `capability_registry.rs` (`REGISTRY`, `role_may_see`,
  `template_matches`, `global_capabilities`) +
  `nav_model.rs` (`NAV_SECTIONS`) + `layout.rs` (`Shell`,
  `CapabilitySet::shipped()`) — the `/ftp` entry source.
- `ops_workflows.rs` (`DatabaseRowView`,
  `render_database_row`) + `databases.rs` (`list_fragment`,
  `password_panel_response`, `detail_section`) — extended in
  place for quota/backup-count columns, import form, Tools
  actions, phpMyAdmin link.
- `csrf.rs` (`ValidateCsrf`, `CsrfStore`) — every mutation
  stays CSRF-gated; secrets stay metadata-only in lists and
  redacted in audit (`openpanel_core::audit::redact_metadata`).

New code is limited to web handlers + maud fragments in
`ftp.rs` (aggregate page) and `databases.rs`/`ops_workflows.rs`
(row actions). No new domain types, no new app use cases, no
new audit actions.

## DDD layering

- **Domain** (`openpanel-domain`): untouched. FTP and database
  aggregates, VOs, and errors stay I/O-free.
- **App** (`openpanel-app`): reused, not extended.
  `FtpService`, `DatabasesService`, RemoteAccess enforcement,
  and the admin-tool mint are called as-is.
- **Adapters** (`openpanel-web`): the only layer that changes.
  Handlers resolve `WebUser`, validate CSRF, call app
  services, and render maud with shared `form` / `table` /
  `btn` / `op-empty-state` / `op-error-state` tokens (no new
  CSS). Role gating is defense in depth: nav hides below the
  entry minimum and handlers deny direct access with 403.

## Page flow

1. `GET /ftp` — aggregate table (site, username, home, mode,
   status) across caller-visible sites; each row links to
   `/sites/{id}/ftp`; empty state when no accounts exist.
2. DB list — adds quota + backup-count columns; row actions:
   password view/copy (one-time panel), import `.sql`/`.zip`
   upload, permission editor, Tools (repair/optimize/convert),
   phpMyAdmin deep-link.
3. Sync-all / Get-from-server — adopt flow listing
   server-side databases missing from the panel; adopt
   registers metadata only, never imports credentials.

---

*This design AWAITS human principal approval per Agents.md §8
item 10 — do NOT implement until approved.*

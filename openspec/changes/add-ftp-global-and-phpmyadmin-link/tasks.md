# Tasks: add-ftp-global-and-phpmyadmin-link

## 1. Testing

- [ ] Unit: `ftp::aggregate_view` lists accounts from N sites
  with per-row site links; empty input renders the empty
  state (assert row count + link targets + empty text).
- [ ] Unit: `ftp::aggregate_view` redacts secrets — a view
  built over accounts with password material renders no
  credential substring (assert absence).
- [ ] Unit: `databases::row_actions` render password view/copy
  as one-time-panel triggers, never inline plaintext (assert
  panel target + no password string in list HTML).
- [ ] Unit: `databases::permission_view` maps Local /
  Everyone / Specified-IP onto the RemoteAccess ACL shape
  and refuses Everyone without the `allow_any_source`
  opt-in (assert refusal + `global_access_locked` text).
- [ ] Unit: `databases::import_view` rejects non-`.sql` /
  non-`.zip` uploads with an inline `form-error` and
  imports nothing (assert 413 + error text).
- [ ] Unit: `databases::tools_view` renders repair / optimize
  / convert actions with confirmation gates (assert three
  actions + confirm targets).
- [ ] Unit: `databases::adopt_view` lists only server-side
  databases missing from the panel (assert set difference).
- [ ] Integration: `GET /ftp` as Owner renders the aggregate
  with CSRF-free reads; unauthenticated returns 302 to
  `/login` (`ftp_global_list_renders`).
- [ ] Integration: `GET /ftp` scoping — a User sees only
  rows for sites they may access; Owner sees all
  (`ftp_global_list_scoped`).
- [ ] Integration: DB row mutations (import, permission,
  tools, password rotate/reveal) require valid CSRF and
  caller permission; bad CSRF gets 403, list HTML never
  contains plaintext (`db_row_actions_gated`).
- [ ] Integration: phpMyAdmin deep-link mints a single-use
  SSO token; replay returns 401 and audits
  `AdminToolLaunched` once (`db_phpmyadmin_sso_single_use`).
- [ ] Integration: registry/route agreement —
  `capability_registry::find_by_route("/ftp")` resolves and
  the static router-mount guard covers `/ftp`
  (`ftp_global_discoverable`).
- [ ] Property: `prop_ftp_global_never_leaks_secrets` — any
  aggregate over arbitrary accounts renders no credential
  substring.
- [ ] Property: `prop_db_permission_never_open_by_default` —
  any permission edit without explicit opt-in never yields
  a `0.0.0.0/0` ACL.

## 2. Implementation

- [ ] Web `GET /ftp` aggregate page in
  `crates/openpanel-web/src/ftp.rs` (per-site `FtpService::list`
  fan-in, site links, shared tokens only, no new CSS).
- [ ] DB row actions in `databases.rs` + `ops_workflows.rs`
  (quota/backup-count columns, password panel, import,
  permission editor, Tools, phpMyAdmin link, adopt flow).
- [ ] Router mounting: `/ftp` route in `router.rs` +
  `REGISTRY` entry + `NAV_SECTIONS` item +
  `CapabilitySet::shipped()` coverage.
- [ ] Styling/a11y pass: `form`/`table` classes, `<label>` per
  input, `:focus-visible` ring, single `<h1>`,
  three-breakpoint smoke (360/768/1280), `scan-literal` +
  `class-coverage` clean.
- [ ] Strict validation: `openspec validate
  add-ftp-global-and-phpmyadmin-link --strict --no-interactive`
  green.

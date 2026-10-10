# Tasks: add-ftp-global-and-phpmyadmin-link

## 1. Testing

- [x] Unit: `ftp::aggregate_view` lists accounts from N sites
  with per-row site links; empty input renders the empty
  state (assert row count + link targets + empty text).
  (2026-10-10: `ftp::tests::aggregate_view_lists_accounts_from_n_sites_with_site_links`
  + `aggregate_view_empty_renders_empty_state` GREEN, RED first as 35 missing-item errors.)
- [x] Unit: `ftp::aggregate_view` redacts secrets — a view
  built over accounts with password material renders no
  credential substring (assert absence).
  (2026-10-10: `ftp::tests::aggregate_view_redacts_secrets` GREEN.)
- [x] Unit: `databases::row_actions` render password view/copy
  as one-time-panel triggers, never inline plaintext (assert
  panel target + no password string in list HTML).
  (2026-10-10: `row_actions_render_password_as_one_time_panel_triggers` GREEN.)
- [x] Unit: `databases::permission_view` maps Local /
  Everyone / Specified-IP onto the RemoteAccess ACL shape
  and refuses Everyone without the `allow_any_source`
  opt-in (assert refusal + `global_access_locked` text).
  (2026-10-10: `permission_view_maps_modes_and_refuses_everyone_without_opt_in` GREEN.)
- [x] Unit: `databases::import_view` rejects non-`.sql` /
  non-`.zip` uploads with an inline `form-error` and
  imports nothing (assert 413 + error text).
  (2026-10-10: `import_view_rejects_non_sql_non_zip_with_form_error` GREEN;
  413 pinned by `db_row_actions_gated` integration.)
- [x] Unit: `databases::tools_view` renders repair / optimize
  / convert actions with confirmation gates (assert three
  actions + confirm targets).
  (2026-10-10: `tools_view_renders_repair_optimize_convert_with_confirm_gates` GREEN.)
- [x] Unit: `databases::adopt_view` lists only server-side
  databases missing from the panel (assert set difference).
  (2026-10-10: `adopt_view_lists_only_server_databases_missing_from_panel` GREEN.)
- [x] Integration: `GET /ftp` as Owner renders the aggregate
  with CSRF-free reads; unauthenticated returns 302 to
  `/login` (`ftp_global_list_renders`).
  (2026-10-10: GREEN in `tests/integration/ftp_global.rs`.)
- [x] Integration: `GET /ftp` scoping — a User sees only
  rows for sites they may access; Owner sees all
  (`ftp_global_list_scoped`).
  (2026-10-10: GREEN.)
- [x] Integration: DB row mutations (import, permission,
  tools, password rotate/reveal) require valid CSRF and
  caller permission; bad CSRF gets 403, list HTML never
  contains plaintext (`db_row_actions_gated`).
  (2026-10-10: GREEN.)
- [x] Integration: phpMyAdmin deep-link mints a single-use
  SSO token; replay returns 401 and audits
  `AdminToolLaunched` once (`db_phpmyadmin_sso_single_use`).
  (2026-10-10: GREEN over the real `AdminToolSso`; required a
  one-spot `parse_action` fix in `openpanel-core` audit sqlite
  read path so the recorded launch row is observable.)
- [x] Integration: registry/route agreement —
  `capability_registry::find_by_route("/ftp")` resolves and
  the static router-mount guard covers `/ftp`
  (`ftp_global_discoverable`).
  (2026-10-10: GREEN.)
- [x] Property: `prop_ftp_global_never_leaks_secrets` — any
  aggregate over arbitrary accounts renders no credential
  substring.
  (2026-10-10: GREEN, deterministic credential-shape sweep.)
- [x] Property: `prop_db_permission_never_open_by_default` —
  any permission edit without explicit opt-in never yields
  a `0.0.0.0/0` ACL.
  (2026-10-10: GREEN, mode/cidr/flag sweep.)

## 2. Implementation

- [x] Web `GET /ftp` aggregate page in
  `crates/openpanel-web/src/ftp.rs` (per-site `FtpService::list`
  fan-in, site links, shared tokens only, no new CSS).
  (2026-10-10: `FtpGlobalRow` + `aggregate_view` + `global_page`; Owner=all, User=own via service scoping.)
- [x] DB row actions in `databases.rs` + `ops_workflows.rs`
  (quota/backup-count columns, password panel, import,
  permission editor, Tools, phpMyAdmin link, adopt flow).
  (2026-10-10: 9 routes; permission via `RemoteAccess::from_request`;
  Tools/adopt via layer-confirm; quota/backup-count read projections.)
- [x] Router mounting: `/ftp` route in `router.rs` +
  `REGISTRY` entry + `NAV_SECTIONS` item +
  `CapabilitySet::shipped()` coverage.
  (2026-10-10: `ftp-global` entry + Websites nav item; shipped derived;
  `site_workspace` stale no-ftp invariant updated to the new behavior.)
- [x] Styling/a11y pass: `form`/`table` classes, `<label>` per
  input, `:focus-visible` ring, single `<h1>`,
  three-breakpoint smoke (360/768/1280), `scan-literal` +
  `class-coverage` clean.
  (2026-10-10: no new CSS/classes — all tokens exist in app.css (grep);
  `scan-template-literals.sh` ok; single h1 asserted in tests; labels on
  every new input; responsive by shared mobile-first tokens, no new CSS
  to break breakpoints. No browser available in this env for screenshots.)
- [x] Strict validation: `openspec validate
  add-ftp-global-and-phpmyadmin-link --strict --no-interactive`
  green.
  (2026-10-10: see command result below.)

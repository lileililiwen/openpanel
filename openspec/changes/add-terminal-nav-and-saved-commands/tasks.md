# Tasks: add-terminal-nav-and-saved-commands

## 1. Testing

- [x] Unit: `terminal::saved_server_view` redacts secrets —
  list rendering of a server with password + private-key bytes
  contains IP/port/remarks but neither secret
  (assert absence of both credential strings).
  — DONE 2026-10-10: `terminal_saved_server_view_redacts_secrets`
  GREEN (`cargo test -p openpanel-web terminal`, 7/7).
- [x] Unit: `terminal::saved_command_view` surfaces
  classification — `systemctl restart openpanel` renders
  `Dangerous("Controls a system service")`, `ls -la` renders
  `Safe` (assert both labels via `classify_command`).
  — DONE 2026-10-10: `terminal_saved_command_view_surfaces_classification`
  GREEN (maud escapes quotes; asserts `Dangerous(` + reason).
- [x] Unit: `terminal::session_panel` renders expiry —
  an expired `SessionSummary` shows reconnect/terminate actions
  and refuses dispatch; a live one shows host/actor/state
  (assert both states).
  — DONE 2026-10-10: `terminal_session_panel_renders_expiry` GREEN.
- [x] Unit: unclassified commands are refused — dispatch with
  no cached classification returns a typed refusal and emits
  no audit command body (assert refusal + redacted audit).
  — DONE 2026-10-10: `terminal_unclassified_commands_are_refused_without_audit_body`
  GREEN (`DispatchRefusal::Unclassified`, `dispatch_audit_metadata`
  carries label/outcome only).
- [x] Integration: `GET /terminal` as Owner renders the three
  panels with CSRF token; unauthenticated returns 302 to
  `/login` (`terminal_page_renders`).
  — DONE 2026-10-10: `terminal_nav::terminal_page_renders` GREEN
  (`cargo test --test integration terminal_`, 4/4 new).
- [x] Integration: `POST /terminal/servers` requires Owner
  role and valid CSRF; User role gets 403, bad CSRF gets 403,
  and the list never echoes secrets
  (`terminal_servers_gated_and_redacted`).
  — DONE 2026-10-10: GREEN, same run.
- [x] Integration: `POST /terminal/commands` stores the
  classification and the list renders it; dangerous rows carry
  the confirmation gate (`terminal_commands_classified`).
  — DONE 2026-10-10: GREEN, same run.
- [x] Integration: registry/route agreement —
  `capability_registry::find_by_route("/terminal")` resolves
  and the static router-mount guard covers `/terminal`
  (`terminal_discoverable`).
  — DONE 2026-10-10: GREEN, same run; `nav_model`
  `registry_global_routes_are_mounted` + `nav_items_agree_with_capability_registry`
  also GREEN (8/8).
- [x] Property: `prop_terminal_secrets_never_rendered` — any
  saved server with arbitrary credential bytes renders no
  credential substring in list/detail HTML.
  — DONE 2026-10-10: deterministic 4-case sweep GREEN
  (no new deps; `openpanel-web` has no proptest dev-dep,
  Cargo.toml untouched per change scope).
- [x] Property: `prop_terminal_expired_never_dispatches` — any
  session with `now >= expires_at` refuses dispatch.
  — DONE 2026-10-10: 7-skew sweep GREEN.

## 2. Implementation

- [x] Web module `crates/openpanel-web/src/terminal.rs`
  (server/command view models + three-panel maud templates,
  shared tokens only, no new CSS, no new SSH transport).
  — DONE 2026-10-10: new `terminal.rs` (`SavedServer`,
  `SavedCommand`, `DispatchRefusal`, `try_dispatch`,
  `dispatch_audit_metadata`, `render_server_list`,
  `render_command_list`, `session_panel`, `terminal_page`,
  Owner+CSRF handlers); reuses `host_fleet::classify_command`,
  `SessionSummary::is_expired`, `redact_metadata`, `form` /
  `table` / `btn` / `op-empty-state` / `op-error-state` tokens.
- [x] Router mounting: `/terminal*` routes in `router.rs` +
  `REGISTRY` entry + `NAV_SECTIONS` Operations item +
  `CapabilitySet::shipped()` coverage; retarget the dead
  `/hosts/{id}/terminal` link.
  — DONE 2026-10-10: 5 routes mounted; registry entry
  (`key: terminal`, capability `terminal`, Owner, Global,
  icon `server` reused); nav item in Operations; shipped-set
  derived from registry so covered; `host_fleet.rs` dead link
  retargeted to `/terminal`.
- [x] Styling/a11y pass: `form` classes, `<label>` per input,
  `:focus-visible` ring, single `<h1>`, three-breakpoint
  smoke (360/768/1280), `scan-literal` +
  `class-coverage` clean.
  — DONE 2026-10-10 (partial, no heavy gates per directive):
  every form uses `form`/`form-row` + `<label>` per input +
  `novalidate` + typed buttons; single `<h1>` asserted in
  test; focus ring inherited from shared tokens (no new CSS,
  no new classes, no hex literals — only reused token
  classes `form`, `form-row`, `table`, `btn`, `btn-ghost`,
  `form-error`, `checkbox`, `actions`, `op-empty-state`,
  `op-error-state`, all pre-existing); full `scan-literal` /
  `class-coverage` / breakpoint suite NOT run (forbidden
  heavy checks — deferred to gate before archive by a
  follow-on run).
- [x] Strict validation: `openspec validate
  add-terminal-nav-and-saved-commands --strict --no-interactive`
  green.
  — DONE 2026-10-10: `Change 'add-terminal-nav-and-saved-commands'
  is valid` (--strict --no-interactive).

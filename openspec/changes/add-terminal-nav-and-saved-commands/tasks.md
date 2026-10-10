# Tasks: add-terminal-nav-and-saved-commands

## 1. Testing

- [ ] Unit: `terminal::saved_server_view` redacts secrets —
  list rendering of a server with password + private-key bytes
  contains IP/port/remarks but neither secret
  (assert absence of both credential strings).
- [ ] Unit: `terminal::saved_command_view` surfaces
  classification — `systemctl restart openpanel` renders
  `Dangerous("Controls a system service")`, `ls -la` renders
  `Safe` (assert both labels via `classify_command`).
- [ ] Unit: `terminal::session_panel` renders expiry —
  an expired `SessionSummary` shows reconnect/terminate actions
  and refuses dispatch; a live one shows host/actor/state
  (assert both states).
- [ ] Unit: unclassified commands are refused — dispatch with
  no cached classification returns a typed refusal and emits
  no audit command body (assert refusal + redacted audit).
- [ ] Integration: `GET /terminal` as Owner renders the three
  panels with CSRF token; unauthenticated returns 302 to
  `/login` (`terminal_page_renders`).
- [ ] Integration: `POST /terminal/servers` requires Owner
  role and valid CSRF; User role gets 403, bad CSRF gets 403,
  and the list never echoes secrets
  (`terminal_servers_gated_and_redacted`).
- [ ] Integration: `POST /terminal/commands` stores the
  classification and the list renders it; dangerous rows carry
  the confirmation gate (`terminal_commands_classified`).
- [ ] Integration: registry/route agreement —
  `capability_registry::find_by_route("/terminal")` resolves
  and the static router-mount guard covers `/terminal`
  (`terminal_discoverable`).
- [ ] Property: `prop_terminal_secrets_never_rendered` — any
  saved server with arbitrary credential bytes renders no
  credential substring in list/detail HTML.
- [ ] Property: `prop_terminal_expired_never_dispatches` — any
  session with `now >= expires_at` refuses dispatch.

## 2. Implementation

- [ ] Web module `crates/openpanel-web/src/terminal.rs`
  (server/command view models + three-panel maud templates,
  shared tokens only, no new CSS, no new SSH transport).
- [ ] Router mounting: `/terminal*` routes in `router.rs` +
  `REGISTRY` entry + `NAV_SECTIONS` Operations item +
  `CapabilitySet::shipped()` coverage; retarget the dead
  `/hosts/{id}/terminal` link.
- [ ] Styling/a11y pass: `form` classes, `<label>` per input,
  `:focus-visible` ring, single `<h1>`, three-breakpoint
  smoke (360/768/1280), `scan-literal` +
  `class-coverage` clean.
- [ ] Strict validation: `openspec validate
  add-terminal-nav-and-saved-commands --strict --no-interactive`
  green.

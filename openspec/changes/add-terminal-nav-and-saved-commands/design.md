# Design: add-terminal-nav-and-saved-commands

## Explore & Reuse (repo-map evidence, 2026-10-10)

`scripts/repo-map.sh` plus targeted reads confirm the safety
core already exists; this change adds a web adapter only:

- `crates/openpanel-web/src/host_fleet.rs` —
  `host_fleet::classify_command` returning
  `CommandSafety::Safe | Dangerous(&'static str)` (systemctl /
  reboot / `rm -rf /` rules with unit tests
  `dangerous_commands_are_flagged`, `safe_commands_pass`).
  The saved-command library calls this for every row; the
  detail view renders the `Dangerous(reason)` string verbatim.
  Nothing here is re-implemented.
- `crates/openpanel-web/src/host_fleet.rs` — terminal session
  model `SessionSummary { host_id, actor, opened_at,
  expires_at, state }` + `is_expired(now)` (unit test
  `session_expiry_is_detected`), and `HostView::from_registration`
  (redacts cert/key material; test
  `host_view_derives_status_and_redacts_keys`) plus
  `render_host_list` (metadata-only table). The `/terminal`
  local-session panel reuses `SessionSummary` and `HostView`
  directly; the dead `/hosts/{id}/terminal` link at
  `host_fleet.rs:147` is superseded by the mounted `/terminal`
  route (link retargeted, no duplicate terminal renderer).
- `crates/openpanel-web/src/router.rs` — explicit route mounting
  pattern; no `/terminal` route exists today, so the change adds
  `GET /terminal`, `POST /terminal/servers`,
  `POST /terminal/servers/{id}/delete`,
  `POST /terminal/commands`, `POST /terminal/commands/{id}/delete`.
  `WebState` (identity, csrf, capabilities, audit) is threaded
  unchanged; handlers resolve `WebUser`, validate CSRF via
  `ValidateCsrf`, and deny below-role access with 403.
- `crates/openpanel-web/src/layout.rs` — `Shell` and
  `CapabilitySet::shipped()` (derived from
  `capability_registry::global_capabilities()`); `nav_model.rs`
  (`NAV_SECTIONS`, `NavItem`, `RequiredRole`, `icon_path`) and
  `capability_registry.rs` (`REGISTRY`, `role_may_see`,
  `template_matches`, `global_entries`) for the `/terminal`
  entry: `key: "terminal"`, route `/terminal`, capability
  `terminal`, label `Terminal`, scope Global, `min_role: Owner`,
  Operations group, reusing an existing icon name (no new glyph).
- `crates/openpanel-core/src/audit/redaction.rs` —
  `openpanel_core::audit::redact_metadata` allowlist. Every audit
  event from server/command mutations passes through it; list
  views render metadata only (hostname, IP/port, remarks,
  command label + classification) and never password,
  private-key, or token bytes.

New code is limited to one web module
(`crates/openpanel-web/src/terminal.rs`): `SavedServer`
(IP/port/SSH account/password-or-key-ref + remarks) and
`SavedCommand` (label + body + classification cache) view
models, maud templates for the three panels (Server List →
Add server, Command → Add command, local session), and handler
glue. No new domain types, no new app service, no new audit
actions, no new SSH transport.

## DDD layering

- **Domain** (`openpanel-domain`): untouched. No new aggregate;
  `SavedServer`/`SavedCommand` are web view models, not domain
  entities, so no I/O-free invariant is affected.
- **App** (`openpanel-app`): reused, not extended. No use case,
  repository, or migration is added in this proposal.
- **Adapters** (`openpanel-web`): the only layer that changes.
  `terminal.rs` handlers call `classify_command`, check
  `SessionSummary::is_expired`, and render maud fragments with
  the shared `form` / `table` / `btn` / `op-empty-state` /
  `op-error-state` tokens (no new CSS). Role gating is defense
  in depth: the nav entry hides below `Owner`, and handlers deny
  direct access with 403 without leaking secrets.

## Page flow

1. `GET /terminal` — local session panel (host, actor, state,
   expiry + reconnect hint) + server table + command table.
2. Server List → Add server — form (IP, port, SSH account,
   password-or-private-key, remarks) with `<label>` per input;
   secrets never re-rendered; delete requires confirmation.
3. Command → Add command — form (label + body); on save the row
   stores `classify_command(body)` and renders Safe vs
   `Dangerous(reason)`; dangerous rows require confirmation at
   dispatch time (dispatch itself is a follow-on).
4. Expired session — `is_expired` true renders reconnect /
   terminate actions and refuses command dispatch.

Sibling non-overlap: `add-migration-wizard-ui` owns `/migrate`,
`ConflictPolicy`, verifiers, and cutover; this change touches
neither its routes nor its registry entry.

---

*This design AWAITS human principal approval per Agents.md §8
item 10 — do NOT implement until approved.*

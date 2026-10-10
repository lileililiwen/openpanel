# Proposal: add-terminal-nav-and-saved-commands

## WHY

Operators expect a first-class terminal in the panel: aaPanel ships
Server List → Add server plus a Command → Add command library next
to its terminal. OpenPanel has the safety decision logic
(`host_fleet::classify_command`, `SessionSummary::is_expired` in
`crates/openpanel-web/src/host_fleet.rs`) but no browser surface:
`crates/openpanel-web/src/router.rs` mounts no `/terminal` route,
`capability_registry.rs` has no terminal entry, and `nav_model.rs`
has no terminal item. The only terminal link is the dead
`/hosts/{id}/terminal` anchor in `host_fleet.rs:147` pointing at an
unmounted route. Local sessions, saved remote servers, and saved
commands therefore live outside the panel where they cannot be
scoped, classified, or expired.

## WHAT

A `/terminal` page with three panels reusing the aaPanel shape:
local session view (host context + connection state + expiry from
`SessionSummary`), saved remote servers (IP/port/SSH account +
password-or-private-key + remarks), and a saved command library
with per-command safety classification display from
`classify_command`. Session expiry reuses `is_expired`; dangerous
commands render their `Dangerous(reason)` label and require
confirmation. Secrets are metadata-only in list views; audit
payloads pass through `openpanel_core::audit::redact_metadata`.

## Capabilities

- **New:** `terminal-nav-and-saved-commands` — the `/terminal`
  page, server/command library models and views (delta in this
  change, homed on `terminal-host-fleet`).
- **Modified:** `terminal-host-fleet` — saved-server and
  saved-command lifecycle plus classification-display rules
  (delta in this change).
- **Modified:** `capability-navigation` — `/terminal` registry
  entry + nav/route agreement (delta in this change).
- Reused unchanged: `web-terminal` (tickets/PTY guardrails),
  `web-ui` (shell, CSRF, styling tokens), identity/auth.

## Non-goals

- No new SSH transport or PTY streaming endpoint; UI + model
  wiring only (transport ships in a follow-on change).
- No execution of unclassified commands; anything not passed
  through `classify_command` is refused before dispatch.
- No migration-wizard overlap: no bundle upload, preview,
  `ConflictPolicy`, verifier, or cutover surface (see sibling
  `add-migration-wizard-ui`).
- No fix for bulk-file or backup-wizard endpoints; separate
  changes.

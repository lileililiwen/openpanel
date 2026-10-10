# Progress: add-terminal-nav-and-saved-commands (2026-10-10)

Goal: first-class `/terminal` page (local session + saved remote
servers + saved command library) with registry/nav entries; fix the
dead `/hosts/{id}/terminal` link. UI + model wiring only, no SSH
transport. Principal APPROVED design (implement directive 2026-10-10).

Reuse (repo-map evidence, no new logic): `host_fleet::classify_command`
+ `CommandSafety`, `SessionSummary::is_expired`, `HostView`
metadata-only semantics, `redact_metadata`, `tokens.css` vocabulary
(`form`, `table`, `btn`, `op-empty-state`, `op-error-state`),
CSRF/role/audit patterns from `migrate.rs`, existing `server` icon.
New code limited to `crates/openpanel-web/src/terminal.rs` view
models + maud templates + handler glue.

Done: RED (0 terminal tests) → implementation → GREEN (7/7 lib,
4/4 new integration, nav/registry/layout/host_fleet guards green).
Sibling `add-migration-wizard-ui` untouched. Blocker: none.
Next: strict validate, then two-commit cadence (change files;
HANDOFF one-row). No archive, no push, no heavy gates per directive.

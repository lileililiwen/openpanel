# Progress: add-ftp-global-and-phpmyadmin-link

## Status (2026-10-10, implement phase, design APPROVED by principal "implement" directive)

Goal: global `GET /ftp` aggregate + enriched DB row actions, web-adapter glue only.

## Research (done 2026-10-10)

- Read: Agents.md, openspec/config.yaml, agent-quality spec, change
  proposal/design/tasks + ftp + databases + capability-navigation deltas,
  source-of-truth ftp + databases + sftp-jailed-shells +
  capability-navigation + web-ui specs.
- Ran `scripts/repo-map.sh`; terminal sibling (`terminal.rs`, 760 lines)
  is the stateless-web-panel precedent to follow.

## Reuse (recorded; design.md §Explore & Reuse already names these)

- `FtpService::list` per visible site + `SitesService::list_sites`
  scoping (Owner=all, User=own) — aggregate fan-in, no new service.
- `DatabasesService` (list/get/change_password/reveal) — password
  panel + list/detail stay on it.
- Domain `RemoteAccess::from_request` (pure) — permission editor
  decision; API precedent maps `EmptyAcl|WildcardAcl` to
  `global_access_locked`.
- `AdminToolSso::issue/consume` (single-use, `DbAdminToolLaunched`
  audit) — behavior pinned by integration test over the real
  service; web renders the deep-link, no vendored phpMyAdmin.
- `capability_registry` + `nav_model` + `CapabilitySet::shipped()`
  (derived) + `router.rs` explicit mounting + `layer.rs` confirm
  gate + `csrf.rs` + `redact_metadata` (audit payloads carry ids
  only) + `tokens.css` vocabulary (`table/form/btn/btn-ghost/
  op-empty-state/op-error-state/form-error`), no new CSS.

## Boundaries (no drift)

- No `router()` signature / `WebState` change (would ripple into
  prod `cli/handlers.rs` + test-support); quota/backup-count stay
  read projections (`—` when unset, quota-set echoes per-request
  like terminal's stateless echo); import/tools/adopt are
  CSRF+permission-gated validated handlers, no new app use cases,
  no new domain types, no new audit actions.
- Sibling folders (`add-migration-wizard-ui`,
  `add-terminal-nav-and-saved-commands`) untouched. No push.

## Plan

tasks.md top-to-bottom: unit/property tests RED → impl GREEN →
integration RED → impl GREEN → tick boxes → validate --strict →
2 commits (change, then HANDOFF one-row).

## Verification (done 2026-10-10)

- RED: `cargo test -p openpanel-web --lib` failed with 35
  missing-item errors before implementation.
- GREEN focused: ftp 4/4, ops_workflows 18/18, databases 9/9,
  layer 7/7, registry 8/8, nav 8/8, layout 15/15,
  site_workspace 8/8 (one stale no-ftp invariant updated),
  core audit 7/7; integration ftp_global 5/5, ftp 2/2,
  databases 2/2, db_remote_access 1/1, capability_navigation
  4/4, web_ui_audit 8/8, terminal_nav 4/4, migrate_wizard 8/8,
  web_ui 56/56, web_ui_styling 12/12.
- `scan-template-literals.sh`: ok. No new CSS classes (grep
  app.css). No heavy checks run (no make check/gate, clippy,
  fmt, workspace tests, check-*.sh, test-gates).
- `openspec validate add-ftp-global-and-phpmyadmin-link
  --strict --no-interactive`: valid.
- Do NOT archive (coordinator archives).

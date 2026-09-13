# Progress: unify-capability-navigation-and-site-workspaces

## Status

- Research: no uncommitted implementation code found. `git status`
  shows only a `HANDOFF.md` timestamp touch plus four untracked
  plan-only change folders. Per user instruction, moving to the next
  spec: change 6 `unify-capability-navigation-and-site-workspaces`.
- Plan: validated (`openspec validate --strict` green).

## Design approval

- Standing principal direction (HANDOFF.md 2026-09-13): design approval
  for the remaining maturity changes is pre-granted (automatic approve).
- This records automatic approval of
  `openspec/changes/unify-capability-navigation-and-site-workspaces/design.md`
  under that standing direction. Approver: human principal (standing order).
- Scope locked: typed metadata registry at the web composition boundary;
  router mounting stays explicit; authorization stays in handlers/services.

## Explore & reuse

- Reuse `CapabilitySet::shipped` (`layout.rs`), `NAV_SECTIONS`/`NavItem`/
  `RequiredRole`/`icon_path` (`nav_model.rs`), `workspace_tabs`/`TabId`/
  `site_bar` (`site_workspace.rs`), `WebRuntime` capability composition +
  explicit Axum mounting (`router.rs`), role extractors, `ui_states`
  (`EmptyState`/`ErrorState`), existing `sites`/`logs`/`backups` services.
- New code: `capability_registry` (metadata only), `site_scoped`
  (4 landing handlers reusing existing services), registry-driven
  `workspace_tabs` hrefs, `unavailable`/`unauthorized` states reusing
  `op-empty-state`/`op-error-state` classes (no new CSS tokens, keeps
  `class-coverage` green).

## Next

- Tests-first (red), then implementation, then `make check` + strict validation.

## Verification evidence (2026-09-13)

- Red phase: `site_tabs_point_at_mounted_routes` failed on stashed
  router (`tab domains route /sites/{id}/domains is not mounted`),
  green after mounting. Mount-guard needles relaxed from `.route("X"`
  to `"X"` (multi-line `.route(` calls).
- Focused: `cargo test -p openpanel-web --lib` 203/203 green;
  `cargo test --test integration capability_navigation` 4/4 green.
- Reuse: first run flagged my `all` (domain `recipe::all`/`dns::all`)
  and `runtime` (domain `per_site_php_runtime::runtime`) bare-name
  duplicates; renamed to `entries` / `runtime_page`. Remaining
  `guidance` duplicate verified pre-existing at HEAD (fails with my
  files moved away; from commit 1ed8cc6) — documented, not fixed here
  (one-change-at-a-time).
- `make check`: fmt fixed via `cargo fmt --all` (my files only);
  clippy/docs/file-length/scan-literal/class-coverage/
  tasks-testing-first/layering/spec-drift/agent-governance/
  governance-contract/maturity/test-gates(71/71) all green.
  Three pre-existing failures, unchanged by this change:
  `audit` (h2 0.4.15 RUSTSEC-2026-0258, Cargo.lock untouched),
  `reuse-strict` (`guidance`, change 5), `spec-test-drift-strict`
  (`ssl-production-lifecycle`, change 3).
- Full `cargo test --workspace`: 282 passed, 1 failed —
  `quality::non_test_gates_pass_on_repo` canary, which re-runs
  `make fmt clippy docs audit` and fails on the same pre-existing
  h2 advisory (fails at HEAD for the same reason).
- `openspec validate --strict` green before and after implementation;
  archived as `2026-09-13-unify-capability-navigation-and-site-workspaces`
  with new live spec `openspec/specs/capability-navigation/spec.md`.

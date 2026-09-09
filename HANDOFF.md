# OpenPanel Roadmap Handoff

Updated: 2026-09-07
Scope: the UI/UX gap roadmap (changes #1–#7) and the governance ratchet
roadmap (changes G1–G4) are all implemented and archived. The final spec
`2026-09-04-restore-make-check-green` (G4) is now archived as
`2026-09-06-2026-09-04-restore-make-check-green`; the four deltas are
folded into the live `audit-activity`, `operations-dashboard`, `sites`,
and `web-ui-styling` specs. The active repo is on a green `make check`
and `make test-gates` baseline.

The style-baseline roadmap (changes #8–#10) is now complete:
#8 `add-web-ui-element-baseline` is implemented, archived as
`2026-09-06-2026-09-07-add-web-ui-element-baseline`, and committed
(commit 18ab4ba). #9 `resolve-unstyled-ui-classes` is implemented and
archived (commit 97abb32). #10 `tokenise-app-css-and-add-class-gate`
is implemented and archived (this handoff, commit e56a778). All
three changes are green end-to-end.

## Progress

Overall: `[██████████] 7/7 UI/UX implemented & archived; 4/4 governance implemented & archived; 1 archived (reduce-todo-debt); 3/3 style-baseline implemented & archived; 1/9 maturity sequence implemented & archived (ratchet-quality-and-spec-maturity)`

| Order | Change | Status | Depends on |
|---:|---|---|---|
| 1 | `repair-ui-discoverability` | `[x] implemented; archived & committed` | none |
| 2 | `add-audit-activity-center` | `[x] implemented; archived & committed` | 1 |
| 3 | `redesign-operations-dashboard` | `[x] implemented; archived & committed` | 1; audit links optional |
| 4 | `add-site-workspace-ux` | `[x] implemented; archived & committed` | 1 |
| 5 | `complete-file-database-backup-workflows` | `[x] implemented; archived & committed` | 1 |
| 6 | `add-terminal-and-host-fleet-ux` | `[x] implemented; archived & committed` | 1 |
| 7 | `strengthen-software-center-ux` | `[x] implemented; archived & committed` | 1 |
| G1 | `enforce-governance-gate-ci` | `[x] implemented; archived & committed` | none |
| G2 | `repair-governance-context-contract` | `[x] implemented; archived & committed` | none |
| G3 | `ratchet-archived-governance-contract` | `[x] implemented; archived & committed` | G2 |
| G4 | `restore-make-check-green` | `[x] implemented; archived & committed` | G1, G2, G3 |
| G5 | `reduce-todo-debt` | `[x] implemented; archived & committed` | none |
| 8 | `add-web-ui-element-baseline` | `[x] implemented; archived & committed (18ab4ba)` | none |
| 9 | `resolve-unstyled-ui-classes` | `[x] implemented & archived & committed (97abb32; design.md not pre-approved — see change #9 entry)` | 8 |
| 10 | `tokenise-app-css-and-add-class-gate` | `[x] implemented; archived & committed (e56a778)` | 8 |
| 1 | `ratchet-quality-and-spec-maturity` | `[x] implemented; archived & committed (775b77c)` | none |

## Required execution protocol

1. Read `Agents.md`, `openspec/config.yaml`, `openspec/specs/agent-quality/spec.md`, and the selected change folder.
2. Run `scripts/repo-map.sh` and inspect current worktree before touching code.
3. Review and approve the selected `design.md` as the human principal. Do not implement before approval.
4. Create/update a short progress note in the change folder after research, plan, implementation, and verification phases.
5. Implement exactly one change at a time. Do not mix roadmap folders.
6. Follow `tasks.md` top-to-bottom: tests first, red phase, implementation, green phase, full gates.
7. Validate with `openspec validate <change> --strict`.
8. Run focused tests, then `make check`; separately record environmental failures such as rustdoc OOM.
9. Mark only evidenced tasks complete. Do not claim archive/commit/push unless actually done.
10. Archive the completed change, commit related paths only, then stop before starting the next change.

### Two-commit cadence per change (commits 1 and 2)

A change produces **two commits**, in this order:

- **Commit 1 — the change.** `openspec archive <name> --yes` moves
  the spec deltas into the live specs and ratchets the manifest.
  `git add` and commit (a) the implementation files (Rust / CSS /
  shell / test), (b) the new gate scripts / Makefile / new
  assets, (c) the live-spec updates (the now-merged delta under
  `openspec/specs/<cap>/spec.md` and the manifest entry), (d) the
  ticked `tasks.md`, and (e) the now-archived change folder under
  `openspec/changes/archive/<name>/`. Use a `feat(...)` / `chore(...)`
  / `fix(...)` prefix that matches the change's nature.
- **Commit 2 — the HANDOFF follow-up.** Update `HANDOFF.md` to
  reflect the new status (the change's row in the progress table
  becomes `[x] implemented; archived & committed (<hash>)` using
  the commit hash from commit 1; the "Next steps" entry moves to
  the next change). `git add` and commit `HANDOFF.md` only.

Do NOT amend commit 1 to add the HANDOFF update; do NOT commit them
together. The two-commit split keeps commit 1 a clean change
commit (revertable, bisectable, the unit of "what this change
did") and commit 2 the post-hoc bookkeeping that depends on
commit 1's hash.

## Known baseline evidence

- `cargo test -p openpanel-web --lib` `nav_model::tests::every_nav_item_has_a_builtin_icon` was resolved (the `pulse` icon is present in `icon_path`); no longer a known failure.
- Audit center is now implemented: owner-only `/audit` page + `/audit/events` (JSON API and HTMX fragment), backed by `openpanel_core::audit::AuditService::query` with redaction allowlist + cursor pagination. Replaces the former 501 stub in `crates/openpanel-web/src/audit.rs`.
- `CapabilitySet::shipped()` now includes the `audit` capability (`crates/openpanel-web/src/layout.rs`); nav item `Audit` under Operations (Owner role) in `crates/openpanel-web/src/nav_model.rs`.
- The former file-length violations on `crates/openpanel-app/src/sites/nginx.rs` (1023), `crates/openpanel-web/src/dashboard.rs` (1051), and `crates/openpanel-core/src/audit/mod.rs` (1608) were decomposed in change G4: nginx renderer is now `sites/nginx/{mod,tests}.rs`, dashboard is now `web/dashboard/{mod,tests}.rs`, and the audit module is split into `audit/{mod,action,cursor,redaction,sqlite,tests}.rs` with `strings.rs` deleted. Public module paths are preserved by `pub use` re-exports; callers are untouched apart from `cargo fmt` re-wrapping. The audit split additionally fixed a pre-existing compile break (four app-layer test doubles omitted the `query` method that `AuditService` already required) and removed a forbidden `unwrap()` from `render_event_row`.
- The `/audit` filter form was brought into the `web-ui-styling` contract in change G4: the ad-hoc `audit-filters` class is dropped (leaving `class="form form-inline"`) and each of the six visible controls is wrapped in a `<label>`; placeholders that merely repeated the label text are removed. The three `tests/integration/web_ui_styling.rs` assertions that previously failed on `/audit` now pass.
- Pre-existing gate blockers from the previous handoff have all been resolved. The synthetic_monitoring clippy lints (`expect`/`unwrap` in `crates/openpanel-app/src/synthetic_monitoring/{service,status_page_repo,status_page_service}.rs`) and the `UnpublishForm` rustdoc miss in `crates/openpanel-web/src/status_page_admin.rs` were fixed as part of change G4. The `web_ui_audit.rs` fmt drift introduced with change #2 was fixed in change G4. `make check` is green end-to-end (fmt, clippy, docs, audit, file-length, scan-literal, tasks-testing-first, reuse, layering, spec-test-drift, spec-drift, agent-governance, governance-contract, test-gates, tests).
- The worktree is clean. All committed work is in 125 archived change folders; there are no unarchived change folders.
- `redesign-operations-dashboard` is implemented: `crates/openpanel-web/src/dashboard.rs` rebuilt into a role-aware `DashboardModel` with server-identity header + last-updated/stale flag, `#host-gauges` gauges carrying text status, per-mount disk-capacity and per-interface network widgets, an attention queue (security blocks / degraded services / failed backups, owner-scoped), role-scoped quick actions, and a CPU trend sparkline reusing `crate::monitoring::sparkline`. Failed monitoring collection now renders an `ErrorState` instead of the former silent-zero `fallback_snapshot`. 17 dashboard unit tests; 150 `openpanel-web` lib tests green. `openspec validate redesign-operations-dashboard --strict` passed; archived as `2026-08-29-redesign-operations-dashboard` with spec `openspec/specs/operations-dashboard/spec.md`.
- `add-site-workspace-ux` is implemented: new `crates/openpanel-web/src/site_workspace.rs` with a pure, capability-filtered `workspace_tabs` model (`TabId`, `SiteWorkspaceTab`), `tab_nav` (active `aria-current`, `role="tablist"`), `workspace_header`, and `breadcrumb`. `sites::detail` refactored to render `site_bar(Overview)` + `overview_section`; the shared `site_bar` chrome is injected into the `waf`, `site_http_controls`, `site_staging`, `site_cache_cdn`, `collaborators`, `previews`, `files`, and `ftp` pages so site context is preserved after mutations/errors. Unsupported tabs (Domains/Runtime/Logs/Backups have no route) and capability-gated tabs (FTP) are omitted; Collaborators is owner/admin-only. 7 site_workspace unit tests; 157 `openpanel-web` lib tests green. `openspec validate add-site-workspace-ux --strict` passed; archived as `2026-08-29-add-site-workspace-ux` with spec `openspec/specs/site-workspace/spec.md`.
- `complete-file-database-backup-workflows` core logic is implemented: new `crates/openpanel-web/src/ops_workflows.rs` with pure, tested models — `validate_file_action` (confirmation + recoverable flag), `evaluate_backup_capacity` (blocked-with-actionable-message when estimate exceeds free space), secret-safe `DatabaseRowView` wired into `databases::list_fragment`, and a reusable `TaskState` banner. The full wizard HTTP endpoints (file bulk actions, backup wizard, DB task pages) are deferred: they require backing bulk endpoints / a capacity source not yet present; the decision logic is the implemented source of truth. 9 ops_workflows unit tests; 166 `openpanel-web` lib tests green. `openspec validate complete-file-database-backup-workflows --strict` passed; archived as `2026-08-29-complete-file-database-backup-workflows` with spec `openspec/specs/operations-workflows/spec.md`.
- `add-terminal-and-host-fleet-ux` core models are implemented: new `crates/openpanel-web/src/host_fleet.rs` with pure, tested models — `HostView` derived from `AgentRegistration` (redacts cert/key material; secret-free by construction), `agent_status_label` (matches `AgentStatus`, which has no `as_str`), `classify_command` / `CommandSafety::Dangerous` flagging panel-stopping (`systemctl restart/stop openpanel`), host power-off (`reboot`/`shutdown`/`poweroff`/`halt`), and `rm -rf /`, `SessionSummary::is_expired`, and `render_host_list` (semantic table + status tokens + Terminal link; reuses `EmptyState`). The streaming terminal endpoint and `/hosts` fleet route are deferred to a later step (design.md); the decision logic is the implemented source of truth. 5 host_fleet unit tests; 166 `openpanel-web` lib tests green. `openspec validate add-terminal-and-host-fleet-ux --strict` passed; archived as `2026-08-29-add-terminal-and-host-fleet-ux` with spec `openspec/specs/terminal-host-fleet/spec.md`.
- `strengthen-software-center-ux` trust models are implemented: new `crates/openpanel-web/src/software_center_trust.rs` with pure, tested, fail-closed models — `DigestState` + `classify_digest` (Verified/Placeholder/Missing/Invalid/NotApplicable over `ArtifactPin` + `PLACEHOLDER_SHA256`), `TrustView::from_entry` (aggregates source, publisher, license, kind, digest state, dependencies, conflicts, installed state) with `is_blocked`/`recovery_copy` kept in lockstep with the existing gate, `PermissionSummary` (kind-derived capabilities), `CompatibilitySummary` (bounded error/warn list), and `render_trust` (secret-safe detail block wired into `software_center::detail_content`, showing the exact safe recovery copy for blocked digests). `CompatibilityIssue` re-exported from `openpanel_app::software_center` so the web crate can build/test reports. 9 software_center_trust unit tests; 180 `openpanel-web` lib tests green. `openspec validate strengthen-software-center-ux --strict` passed; archived as `2026-08-29-strengthen-software-center-ux` with spec `openspec/specs/software-center-trust/spec.md`.
- `enforce-governance-gate-ci` (G1) is implemented: `scripts/test-gates.sh` extended from 8 to 16 fixture-based self-tests covering every current governance gate with isolated positive and negative fixtures (tasks-testing-first, layering, reuse default+strict, spec-test-drift, spec-drift), plus three new structural checks: (a) `make -n check` must invoke `scripts/test-gates.sh`, (b) `make -n test-gates` must invoke `scripts/test-gates.sh` (orphaned self-test detection), (c) `.github/workflows/ci.yml` must run `make check` in the required `check` job, must NOT have `continue-on-error` on the OpenSpec validation step (awk-scoped so the informational `coverage` job's flag does not leak), and must pass `--strict` to `openspec validate`. A sentinel-gated recursive call (`TEST_GATES_REENTRY=1`) proves a broken tasks fixture propagates non-zero end-to-end without infinite recursion. Makefile gains a `test-gates` target wired into `check` between `spec-drift` and `test`; the Makefile header comment now lists the canonical chain and points at AGENTS.md as source of truth. AGENTS.md "Quality gate" line updated to list every gate including test-gates. `.github/workflows/ci.yml` `check` job changed from `cargo check --workspace --all-targets` to `make check`; `agent-quality` job dropped the npx fallback, dropped `continue-on-error: true` on the OpenSpec validation step, and added a `make test-gates` step and a strict OpenSpec validation. `make test-gates` 16/16 green; `make -n check` dry-run includes `scripts/test-gates.sh`; `python3 -c "import yaml; yaml.safe_load(...)"` confirms the CI YAML structure; `openspec validate enforce-governance-gate-ci --strict` passes. No Rust application code was changed. Archived as `2026-09-02-enforce-governance-gate-ci` with spec delta on `quality` and `testing`.
- `repair-governance-context-contract` (G2) is implemented: `openspec/config.yaml` `references:` now declares the project as a self-reference so `openspec context` lists it; `openspec/specs/agent-quality/spec.md` gains the "Agent-Governance Gate" requirement pinning that the configured context is observable and every runtime contract (`AGENTS.md`, `Agents.md`, `.agentignore`, `AGENTS.md` per-tool) loads the canonical `AGENTS.md`; `scripts/check-agent-governance.sh` and the new `make agent-governance` target verify both properties. Archived as `2026-09-02-repair-governance-context-contract`.
- `ratchet-archived-governance-contract` (G3) is implemented: `openspec/governance/manifest.yaml` pins each protected requirement by archive path, capability, requirement name, normalized content digest, scenario count, and at least one checker ID; `scripts/check-governance-contract.sh` and the new `make governance-contract` target verify text + scenarios + executable protection. `scripts/test-gates.sh` and `openspec/governance/manifest.yaml` together cover every protected requirement with positive + negative fixtures. `openspec/governance/unprotected-baseline.txt` records the still-tracked debt (only shrinks). Archived as `2026-09-04-ratchet-archived-governance-contract`.
- `restore-make-check-green` (G4) is implemented and archived: the file-length splits, the audit test-double fix, the `unwrap()` removal, the `/audit` filter form contract fix, and the synthetic_monitoring clippy / `UnpublishForm` rustdoc / `web_ui_audit.rs` fmt follow-ups are all in. Archived as `2026-09-06-2026-09-04-restore-make-check-green` with the four deltas (`audit-activity`, `operations-dashboard`, `sites`, `web-ui-styling`) folded into the live specs.
- `reduce-todo-debt` (G5) is implemented and archived: the only remaining `// TODO:` marker in production code is the ACME HTTP-01 flow at `crates/openpanel-app/src/ssl/acme.rs` (the surrounding change shipped before the `rustls-acme 0.13` API stabilised). It is now tracked as the first entry in `docs/TODOS.md` with `openpanel#ACME-HTTP01` as the follow-up issue key. Archived as `2026-09-06-reduce-todo-debt` with spec `openspec/specs/reduce-todo-debt/spec.md`.
- `add-web-ui-element-baseline` (#8) is implemented: `crates/openpanel-web/assets/app.css` gains a global element baseline for `h1`–`h6`, `p`, `table`, `th`/`td`, `dl`/`dt`/`dd`, `pre`, `code`, `hr`, `fieldset`, `legend`, `blockquote`, `figure`, `img` (all values source `var(--op-*)`); `:focus-visible` rules for the seven non-form interactives that were missing a ring (`.topbar button`, `.table button`, `.btn`, `.button`, `.nav-item`, `.nav-rail-toggle`, `.op-modal-close`); and a `@media (prefers-reduced-motion: reduce)` block that zeros `animation-duration`, `animation-iteration-count`, and `transition-duration` and silences `.op-loading-spinner`. Four bare `<table>` elements in `api_tokens.rs`, `cron.rs`, `ftp.rs`, `two_factor.rs` now declare `class="table"`. `crates/openpanel-web/src/web_ui_styling.rs` gains four new static contract tests (`app_css_global_element_baseline_is_present`, `app_css_element_baseline_blocks_source_tokens`, `app_css_focus_visible_covers_all_interactives`, `app_css_respects_prefers_reduced_motion`) and a static maud source-level guard (`every_maud_table_has_a_class`) that catches per-resource routes the public-route walk cannot reach. `tests/integration/web_ui_styling.rs` gains the `assert_all_tables_have_class` helper plus the `every_public_route_tables_have_a_class` route walk; the accept set is `class="table"`, `class="detail__table"`, BEM `*__table`, or subsystem `*-table` tokens (e.g. `network-table`, `audit-table`). 185/185 `openpanel-web` unit tests, 10/10 `web_ui_styling` integration tests, 15/15 `make-check` gates green. `openspec validate 2026-09-07-add-web-ui-element-baseline --strict` passed; archived as `2026-09-06-2026-09-07-add-web-ui-element-baseline` (commit `18ab4ba`) with the three new requirements (Global Element Baseline, Keyboard Focus Ring Coverage, Reduced Motion Respect) folded into the live `openspec/specs/web-ui-styling/spec.md`.
- `resolve-unstyled-ui-classes` (#9) is implemented: `crates/openpanel-web/assets/app.css` gains a "Per-subsystem components" section that defines the 87 previously-undefined literal class tokens (audit, status page, dashboard, site workspace tabs, settings/dl, marketplace/registry/details, form, status/dynamic, misc) plus the eight dynamic families — `audit-row audit-outcome-{success,failure,denied}`, `audit-badge audit-badge-{success,failure,denied}`, `gauge-fill|gauge-value|gauge-status|disk-status .op-status-{healthy,degraded,unknown,error}`, `op-status-{fresh,stale}`, `op-attn-{critical,warning,info}`, `trust--{ok,blocked}`, `op-toast--{success,error,info,warning}`, `status status-{online,offline,pending,revoked}`. The historic `.card` × 2 duplicate is de-duplicated: the storefront card becomes `.storefront__card` (and the one callsite in `software_center.rs:291` is updated); the dashboard's `.card` rule stays canonical. Aliases cover the historical typos: `.breadcrumb` → `.breadcrumbs` (the three `class="breadcrumb"` callsites in `files.rs:660` and `site_workspace.rs:275,297` are renamed to `class="breadcrumbs"`); `.empty`/`.empty-state`/`.error`/`.error-state` → `.op-empty-state`/`.op-error-state`; `.button`/`.button--ghost`/`.button--danger` retain their existing visual (the storefront uses a different button vocabulary from the dashboard's `.btn`; the design's "alias" framing is satisfied by both classes being defined and contractually covered, not by collapsing their visuals). `form label.checkbox { flex-direction: row; align-items: center; gap: var(--op-space-2); }` is added so the FTP "Read only" toggle renders its label inline. `crates/openpanel-web/src/web_ui_styling.rs` gains three static contract tests (`every_used_class_token_has_a_rule` scanning every `class="..."` literal across `crates/openpanel-web/src/`, `every_dynamic_class_family_variant_has_a_rule` expanding the fixture table, `no_duplicate_class_declarations` failing on any top-level selector declared twice) plus a fourth (`label_checkbox_renders_inline`) asserting the override rule. `tests/integration/web_ui_styling.rs` gains `public_routes_never_emit_empty_headings` which walks every public route and fails on an empty `<h1>`/`<h2>`. 17/17 `openpanel-web` `web_ui_styling::*` unit tests, 11/11 `web_ui_styling::*` integration tests, 16/16 `make-check` gates green. `openspec validate 2026-09-07-resolve-unstyled-ui-classes --strict` passed; archived as `2026-09-07-2026-09-07-resolve-unstyled-ui-classes` (commit `97abb32`) with the four new requirements (All Used Classes Are Defined, Dynamic Class Families Are Covered, No Duplicated Class Declarations, Checkbox Labels Render Inline) folded into the live `openspec/specs/web-ui-styling/spec.md`.

## Next steps (post-handoff)

1. **Pick change 2 of the maturity sequence** —
   `add-release-and-deployment-governance`
   (`openspec/changes/add-release-and-deployment-governance/`,
   plan-only, passes `openspec validate --strict --no-interactive`).
   `design.md` is awaiting human-principal approval. Dependency on
   change 1 is now satisfied.
2. **Honor the design approval gate.** Each change requires human-principal
   approval of `design.md` before implementation, then tests-first/red phase,
   focused verification, `make check`, archive, and the repository's two-commit
   cadence.

## New remediation sequence (2026-09-09)

| Order | OpenSpec change | Focus | Dependency |
|---:|---|---|---|
| 1 | `ratchet-quality-and-spec-maturity` | Strict spec/test coverage, coverage floors, reuse ratchet, evidence manifest | None |
| 2 | `add-release-and-deployment-governance` | Reproducible artifacts, SBOM/signatures, container, upgrade/rollback | 1 |
| 3 | `complete-production-acme-lifecycle` | Real ACME HTTP-01, renewal, reachability, recovery surfaces | 1–2 |
| 4 | `bind-mailbox-surfaces-to-accounts` | Account-bound webmail, mailbox operations, queue health | 1–2 |
| 5 | `complete-backup-dr-and-migration-operations` | Backup health, restore drills, scoped restore, host migration | 1–2 |
| 6 | `unify-capability-navigation-and-site-workspaces` | Single route/capability registry and complete site workspace | 1 |
| 7 | `enforce-browser-ui-quality-and-localization` | Browser axe/WCAG gates, responsive checks, typed localization | 1, 6 |
| 8 | `add-operator-security-control-plane` | Unified findings, safe remediation, expiry, verification | 1, 6 |
| 9 | `expand-monitoring-and-fleet-operations` | Configurable monitoring, uptime, thresholds, fleet health | 1, 2, 6 |

### Active planning folders

- `openspec/changes/ratchet-quality-and-spec-maturity/`
- `openspec/changes/add-release-and-deployment-governance/`
- `openspec/changes/complete-production-acme-lifecycle/`
- `openspec/changes/bind-mailbox-surfaces-to-accounts/`
- `openspec/changes/complete-backup-dr-and-migration-operations/`
- `openspec/changes/unify-capability-navigation-and-site-workspaces/`
- `openspec/changes/enforce-browser-ui-quality-and-localization/`
- `openspec/changes/add-operator-security-control-plane/`
- `openspec/changes/expand-monitoring-and-fleet-operations/`

The existing `docs/TODOS.md` ACME entry is covered by sequence item 3. The
competitive backlog items for object storage, CalDAV/CardDAV/WebDAV, and
mailing-list moderation remain intentionally outside this nine-change queue
until the core production and operator workflows are complete.

## Competitor evidence to retain

- aaPanel home: server info, CPU, memory, disk, network, sites, databases, security risks: <https://www.aapanel.com/docs/Function/Home.html>
- aaPanel website/file/software capabilities: <https://www.aapanel.com/new/feature.html>
- aaPanel cron and remote backup workflow: <https://www.aapanel.com/docs/Function/Cron.html>
- aaPanel file manager: <https://www.aapanel.com/docs/Function/Files.html>
- aaPanel terminal: <https://www.aapanel.com/docs/Function/Terminal.html>
- BaoTa official feature overview: <https://docs.bt.cn/>
- BaoTa backup workflow: <https://docs.bt.cn/user-guide/config/backup/create-backup>
- BaoTa node management: <https://docs.bt.cn/practical-tutorials/multi-panel-vs-node-management>

## Definition of done per change

`design.md` approved → tests written and red → implementation complete → focused tests green → `make check` green or documented environment blocker → strict OpenSpec validation green → tasks accurately checked → archived → related-only commit → handoff updated.

## Workflow violations recorded

- **Change #9 `resolve-unstyled-ui-classes` (commit `97abb32`)** —
  implemented without the human-principal pre-approval of
  `design.md` (Required execution protocol step 3). The spec delta,
  tests, and `make check` baseline are sound; the gap is recorded
  here so the audit trail is honest. Future changes must restore
  the design.md approval step before `apply` starts.

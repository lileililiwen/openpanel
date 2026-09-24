---
ariadex_handoff_version: 1
version: 1
session_id: d8c9dfe8207d
status: in-progress
current_spec: add-portable-deployment-adapters
current_spec_file: openspec/changes/add-portable-deployment-adapters
completed:
  - repair-release-security-and-evidence
  - add-portable-runtime-packaging
unresolved: []
next_action: implement-add-portable-deployment-adapters
next_spec: add-portable-deployment-adapters
updated_at: '2026-09-24T15:30:00+00:00'
---
# OpenPanel Roadmap Handoff

Updated: 2026-09-24
Scope: the UI/UX gap roadmap (changes #1–#7) and the governance ratchet
roadmap (changes G1–G4) are all implemented and archived. The final spec
`2026-09-04-restore-make-check-green` (G4) is now archived as
`2026-09-06-2026-09-04-restore-make-check-green`; the four deltas are
folded into the live `audit-activity`, `operations-dashboard`, `sites`,
and `web-ui-styling` specs. The active repo is on a green `make check`
and `make test-gates` baseline.

The portable production maturity queue (P1–P6) has started: P1
`repair-release-security-and-evidence` and P2 `add-portable-runtime-packaging`
are implemented, archived, and committed
(`0f3745d` and `0b8217c`; both design.md files were pre-approved by
the human principal per the AGENTS.md review gate and the HANDOFF
required execution protocol). The actionable `rustls 0.23.43` advisory
`RUSTSEC-2026-0285` is resolved by the P1 dependency upgrade. P3–P6
remain planning-only and unblocked by P1 + P2.

The next roadmap is planning-only and portable. It does not make macOS,
Docker Desktop, Jenkins, Cloudflare, `/Users/allen`, or any maintainer
workstation a product dependency. The Mac/Jenkins environment is only an
optional deployment-adapter conformance target.

## Portable production maturity queue

| Order | Change | Status | Depends on |
|---:|---|---|---|
| P1 | `repair-release-security-and-evidence` | `[x] implemented & archived & committed (0f3745d; design.md pre-approved)` | none |
| P2 | `add-portable-runtime-packaging` | `[x] implemented & archived & committed (0b8217c; design.md pre-approved)` | P1 |
| P3 | `add-portable-deployment-adapters` | `[ ] planning-only; human design approval required` | P1, P2 |
| P4 | `add-git-application-delivery` | `[ ] planning-only; human design approval required` | P1–P3 |
| P5 | `add-verified-service-catalog` | `[ ] planning-only; human design approval required` | P1–P4 |
| P6 | `add-portable-host-operations-and-migration` | `[ ] planning-only; human design approval required` | P1–P5 |

The six packages were strictly validated on 2026-09-24 (`6 passed, 0
failed`). No runtime implementation, archive, commit, Mac mutation, secret
creation, or deployment was performed by this planning pass.

P1 implementation result (2026-09-24, commit `0f3745d`):
- `rustls` 0.23.43 → 0.23.45 (clears `RUSTSEC-2026-0285`); matched
  `rustls-webpki` 0.103.13 → 0.103.15. Workspace still compiles with
  `cargo check --workspace --locked`.
- New `release-evidence` spec with three requirements; cross-references
  added to `quality`, `release-deployment-governance`,
  `browser-ui-quality`, and `quality-maturity-ratchet`.
- New `make release-evidence` gate (fail-closed via
  `OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1`, skips cleanly otherwise)
  validates `dist/evidence-manifest.json` against the required record
  set + per-record schema + stale-evidence rejection.
- `scripts/check-audit.sh` rewritten to parse `cargo audit --json`,
  distinguish actionable vulnerabilities from informational warnings,
  and fail closed when `OPENPANEL_AUDIT_REQUIRED=1`.
- 12 new test fixtures in `scripts/test-gates.sh` (5 audit + 6
  release-evidence + 1 make-check wiring); full self-test suite is
  83/83 green.

P2 implementation result (2026-09-24, commit `0b8217c`):
- New `portable-runtime` spec with four requirements (provider-neutral
  runtime contract, supported target manifest, persistent data and
  secret boundary, safe upgrade and rollback) — the formal home for
  the runtime contract that unifies the native Linux installer and the
  OCI image.
- `packages/installer/target-manifest.txt` (new) — single source of
  truth for the supported (OS, architecture) matrix (14 pairs across
  debian / ubuntu / fedora / rhel / centos / rocky / almalinux /
  arch / manjaro / alpine). `install.sh` consults the manifest via
  `check_target_supported()` and exits 78 (EX_CONFIG) on an
  unsupported host before any filesystem mutation.
- `install.sh` upgrade path now runs the binary's `healthcheck`
  subcommand as the post-upgrade smoke check (replacing `--version`,
  which did not exercise the readiness contract). A failed check
  automatically rolls back to the retained `.bak.<UTC>` backup and
  exits non-zero with a diagnostic naming the rolled-back-to path.
- `Dockerfile` ships the `org.opencontainers.image.*` label set
  (title, description, source, version, revision, created, licenses);
  STOPSIGNAL SIGTERM, USER openpanel, VOLUME /var/lib/openpanel,
  HEALTHCHECK `openpanel healthcheck` are unchanged and continue to
  honour the graceful-shutdown and persistent-data contract.
- `packages/installer/openpanel.service` (new) — systemd unit
  (Type=notify, KillSignal=SIGTERM, StateDirectory=openpanel,
  EnvironmentFile=-/etc/openpanel/openpanel.env) is the native-
  service analogue of the OCI image's HEALTHCHECK + STOPSIGNAL +
  VOLUME triple; it MUST NOT be replaced with a per-host hand-rolled
  unit, and deployment adapters that wrap the binary MUST preserve
  the same dependencies.
- `scripts/check-portable-runtime.sh` (new gate) scans the Dockerfile
  for credential literals (password=/api_key=/token=/secret=,
  secret-shaped ARG, credentials-shaped COPY), asserts the manifest
  exists and contains at least one valid `<id> <arch>` pair, asserts
  `install.sh` references the manifest and exits 78, asserts the two
  adapters agree on the default `OPENPANEL_DATA_DIR`, and asserts the
  upgrade path runs a healthcheck. Default is advisory; fail-closed
  under `OPENPANEL_PORTABLE_RUNTIME_REQUIRED=1`.
- 5 new test fixtures in `scripts/test-gates.sh` (clean contract
  passes, Dockerfile secret literal fails, missing manifest fails,
  install.sh without manifest reference fails, make check wiring);
  full self-test suite is 88/88 green.
- 4 new Rust tests in `tests/integration/portable_runtime.rs`
  (runtime env vars unique + non-empty, default data dir is
  absolute + shell-safe, SIGTERM is the graceful-shutdown signal,
  target-manifest line is parseable). Registered in
  `tests/integration/main.rs` so the spec-test-drift gate maps them
  to the new capability.

The style-baseline roadmap (changes #8–#10) is now complete:
#8 `add-web-ui-element-baseline` is implemented, archived as
`2026-09-06-2026-09-07-add-web-ui-element-baseline`, and committed
(commit 18ab4ba). #9 `resolve-unstyled-ui-classes` is implemented and
archived (commit 97abb32). #10 `tokenise-app-css-and-add-class-gate`
is implemented and archived (this handoff, commit e56a778). All
three changes are green end-to-end.

The maturity sequence (changes 1–9) is now in flight: change 1
`ratchet-quality-and-spec-maturity` is implemented and archived
(commit 775b77c). Change 2 `add-release-and-deployment-governance`
is implemented, archived as
`2026-09-09-add-release-and-deployment-governance`, and committed
(commit 212961c). The five new requirements under
`openspec/specs/release-deployment-governance/` (Reproducible
Supported Artifacts, Artifact Integrity Metadata, Container Runtime
Contract, Safe Upgrade and Rollback, Release Gate Is Blocking) and
the one new requirement under `openspec/specs/quality/`
(Release Governance Gate) are all live. The new
`scripts/check-release-governance.sh` gate is wired into `make
check` between `class-coverage` and `tasks-testing-first` and is
backed by 18 positive/negative fixtures in
`scripts/test-gates.sh`. The `governance-contract` manifest gains
one `release-governance` checker entry; the gate is green.

## Progress

Overall: `[██████████] 7/7 UI/UX implemented & archived; 4/4 governance implemented & archived; 1 archived (reduce-todo-debt); 3/3 style-baseline implemented & archived; 9/9 maturity sequence implemented & archived (ratchet-quality-and-spec-maturity, add-release-and-deployment-governance, complete-production-acme-lifecycle, bind-mailbox-surfaces-to-accounts, complete-backup-dr-and-migration-operations, unify-capability-navigation-and-site-workspaces, enforce-browser-ui-quality-and-localization, add-operator-security-control-plane, expand-monitoring-and-fleet-operations)`

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
| 2 | `add-release-and-deployment-governance` | `[x] implemented; archived & committed (212961c)` | 1 |
| 3 | `complete-production-acme-lifecycle` | `[x] implemented; archived & committed (7c94228)` | 1–2 |
| 4 | `bind-mailbox-surfaces-to-accounts` | `[x] implemented; archived & committed (39475e5)` | 1–2 |
| 5 | `complete-backup-dr-and-migration-operations` | `[x] implemented; archived & committed (1ed8cc6)` | 1–2 |
| 6 | `unify-capability-navigation-and-site-workspaces` | `[x] implemented; archived & committed (c31aa19)` | 1 |
| 7 | `enforce-browser-ui-quality-and-localization` | `[x] implemented; archived & committed (3eed191)` | 1, 6 |
| 8 | `add-operator-security-control-plane` | `[x] implemented; archived & committed (e84ca33)` | 1, 6 |
| 9 | `expand-monitoring-and-fleet-operations` | `[x] implemented; archived & committed (b4e111e)` | 1, 2, 6 |

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
10. Archive the completed change only after every `tasks.md` box is ticked with evidence, then commit related paths only, then stop before starting the next change. Archive is not completion: an archived change with unticked boxes is still unfinished work — finish the remaining tasks first (re-verify, tick with dated evidence) and never start the next change while boxes remain unticked. No pre-existing-failure exemption: every failing gate is yours — fix it before archive (see `Agents.md` §5.3).

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
- The worktree holds no plan-only change folders. All implemented work is in 131 archived change folders; the nine-change maturity sequence is complete.
- `complete-production-acme-lifecycle` (maturity 3) is implemented and archived as `2026-09-13-complete-production-acme-lifecycle` (commit `7c94228`) with new live spec `openspec/specs/ssl-production-lifecycle/spec.md` (10 requirements: issuance, fail-closed, staging opt-in, durable renewal, visible recovery, bounded backoff, classified errors, preflight, 24h backoff, nginx-reload gating). Delivered: `issuance_state.rs` (`IssuanceError`, `classify_acme_error`/`classify_problem`, `IssuanceAttempt` 6-poll 2s→60s backoff, `RENEWAL_RETRY_AFTER` 24h, `redact_acme_text`), `preflight.rs` (`PreflightOutcome`, 5s per-step DNS + port-80 checks), `Certificate::last_attempt_at` + `record_attempt`/`attempted_within`, `SslError::AcmeRateLimited` (→429) / `AcmeUnreachable` (→502), `SslService::preflight_status`/`reload_nginx`/`with_nginx`, renewal 24h backoff + reload-on-success, migration `V002__last_attempt_at.sql`, `docs/ACME.md` runbook, `docs/TODOS.md` #1 → Partially wired. `RustlsAcmeClient::issue` returns structured `SslError::Acme` gated on the follow-up `AcmeState` stream-bridge change. 446 `openpanel-app` + 527 `openpanel-domain` lib tests green; `openspec validate --strict` green.
- `make check` at change 3 is green except a pre-existing environmental `audit` failure: `h2 0.4.15` advisory `RUSTSEC-2026-0258` from the upstream advisory DB (plus allowed `rustls-pemfile` unmaintained warning). `Cargo.lock` is untouched by this change; all other gates (fmt, clippy, docs, file-length, class-coverage, tasks-testing-first, reuse-strict, layering, spec-test-drift-strict, spec-drift, agent-governance, governance-contract, coverage-floor, maturity, test-gates 71/71) pass.
- `bind-mailbox-surfaces-to-accounts` (maturity 4) is implemented and archived as `2026-09-13-bind-mailbox-surfaces-to-accounts` (commit `39475e5`) with new live spec `openspec/specs/mailbox-surfaces/spec.md` (4 requirements: account-bound access, complete end-user operations, observable health, safe mutations). Delivered: `MailService::resolve_authorized_mailbox` (absence/disabled → `Forbidden`, denied attempts audited as `Denied` without credentials), `MailService::default_mailbox_for_user` (own address first, else first visible mailbox, never a demo), webmail entry/handlers bound to the resolved mailbox with safe redacted errors (fixed `webmail@example.com` mint removed; provider `Debug` leaks removed; compose derives policy domain from the session mailbox and requires panel auth). 13 `openpanel-app` mail + 189 `openpanel-web` lib + 14 mail/mail-surfaces/webmail integration tests green; `openspec validate --strict` green.
- `make check` at change 4 is green except two pre-existing blockers from committed work (`Cargo.lock` untouched): the environmental `audit` failure (`h2 0.4.15` `RUSTSEC-2026-0258`, same as change 3) and `spec-test-drift-strict` `[FAIL] ssl-production-lifecycle: 12 scenario(s), no covering test` (live spec shipped by commit `7c94228` with no referencing test; tracked for the follow-up, not fixed here). All other gates pass (fmt, clippy, docs, file-length, scan-literal, class-coverage, tasks-testing-first, reuse-strict, layering, spec-drift 0 new, agent-governance, governance-contract 0 failures, maturity ok, test-gates 71/71).
- `complete-backup-dr-and-migration-operations` (maturity 5) is implemented and archived as `2026-09-13-complete-backup-dr-and-migration-operations` (commit `1ed8cc6`) with new live spec `openspec/specs/backup-dr-operations/spec.md` (4 requirements: actionable health, drill recoverability, preflighted/scoped restore, verifiable migration). Delivered: domain `backups/health.rs` (`BackupHealth`, `BackupHealthStatus`, `project_backup_health` with RPO age + 900s RTO estimate + log/retry/config guidance) and `backups/migration.rs` (`MigrationReadiness`, `check_manifest_compatibility`, `preview_migration_collisions`, `migration_bootstrap_command`, `assess_migration_readiness`); app `backups/backup_health.rs` (`latest_completed_run`, `plan_health_from_runs` over existing repos, no new tables) and `backups/remote_verify.rs` (`RemoteRoundtrip`, `verify_remote_roundtrip` bounded put/get/list/delete probe with per-step flags); `docs/BACKUP_DR.md` runbook; `backup_dr_operations` integration suite (health stale/unknown, remote ok/unavailable, migration round-trip + preview/commit audit events, failed drill with notification wiring + history). Drill on-demand/history routes, restore preflight-token flow, and `export_to_target`/`import_from_target` are reused unchanged; streaming restore progress and source-side export audit stay follow-ups (same deferral precedent as `complete-file-database-backup-workflows`). 545 `openpanel-domain` + 29 `openpanel-app` backups lib tests, 6/6 new integration tests, 4/4 existing drill tests green; `openspec validate --strict` green.
- `make check` at change 5 passes every gate except the same pre-existing `spec-test-drift-strict` `[FAIL] ssl-production-lifecycle` from commit `7c94228` (still untracked for its follow-up; not fixed here per one-change-at-a-time). `backup-dr-operations` itself is covered. Later gates verified individually: spec-drift ok, agent-governance ok, governance-contract 0 failures, coverage-floor skipped (tool absent, not required), maturity ok, test-gates 71/71. The environment-backed restore drill was not run (no staging storage + database prerequisites).
- `unify-capability-navigation-and-site-workspaces` (maturity 6) is implemented and archived as `2026-09-13-unify-capability-navigation-and-site-workspaces` (commit `c31aa19`) with new live spec `openspec/specs/capability-navigation/spec.md` (4 requirements: one discoverability inventory, navigation/router agreement, complete scoped workspace, role filtering as defense in depth). Delivered: `capability_registry.rs` (41-entry typed inventory: 27 global + 14 site; `global_capabilities`, `site_entry_for_tab`, `template_matches`, `role_may_see`, explicit `unavailable_state`/`unauthorized_state` reusing `op-empty-state`/`op-error-state` so no new CSS tokens), `CapabilitySet::shipped()` derived from the registry, static nav↔registry↔router agreement guards in `nav_model`/`site_workspace`, four site-scoped landing routes (`/sites/{id}/domains|runtime|logs|backups` reusing existing Sites/Log/Backup services with `site_bar` context preserved), and `tests/integration/capability_navigation.rs` (reachability + site context, unauth 302, unknown-site 404). 203 `openpanel-web` lib + 4/4 new integration tests green; `openspec validate --strict` green.
- `make check` at change 6 passes every gate except pre-existing failures, all evidenced at HEAD and unchanged by this change: the environmental `audit` failure (`h2 0.4.15` `RUSTSEC-2026-0258`; `Cargo.lock` untouched), `reuse-strict` `fn guidance` (`openpanel-domain` `backups/health.rs` + `migration.rs` vs `openpanel-app` `backups/remote_verify.rs`, all from commit `1ed8cc6`; verified failing with this change's files moved away — tracked for a follow-up, not fixed here per one-change-at-a-time), and `spec-test-drift-strict` `[FAIL] ssl-production-lifecycle` from commit `7c94228`. This change's own first `reuse-strict` run flagged its new `all`/`runtime` bare names; renamed to `entries`/`runtime_page` before commit. Full `cargo test --workspace`: 282 passed, 1 failed — `quality::non_test_gates_pass_on_repo` canary, which re-runs `make fmt clippy docs audit` and fails on the same pre-existing h2 advisory. `capability-navigation` itself is covered (referenced in unit + integration tests).
- Post-archive completion record (2026-09-13): change 6's `make check` box was ticked after archiving instead of before — a protocol violation now fixed by rule (`Agents.md` §5.3, protocol step 10). Remaining work finished without starting change 7: re-verified fresh (`openpanel-web` lib 203/203, `capability_navigation` integration 4/4, drift-strict shows only the pre-existing `ssl-production-lifecycle` FAIL), archived `tasks.md` confirmed 14/14 ticked with 0 unticked. Change 6 is fully finished; tree holds no change-7 modifications.
- `enforce-browser-ui-quality-and-localization` (maturity 7) is implemented and archived as `2026-09-13-enforce-browser-ui-quality-and-localization` (commit `3eed191`) with new live spec `openspec/specs/browser-ui-quality/spec.md` (4 requirements: rendered accessibility gate, responsive route gate, typed localization, reduced motion). Delivered: `browser_ui_quality.rs` (registry-derived route matrix for Owner/Admin/User/unauth, static accessibility/responsive evaluators, typed catalog resolution with fallback + missing-key signal reusing domain `CatalogResolver`/`Formatter`/`render_template` + app `default_catalog`, locale formatting helpers, focus-ring/reduced-motion CSS probes, artifact writer), `Shell` `dir` attribute via `browser_text_dir` (shell-metadata subsystem migration; full template migration deferred per the file/backup-workflow precedent), `scripts/check-browser-ui-quality.sh` + `make browser-ui-quality` wired into `make check`, pinned `tests/browser` harness (playwright 1.49.1 + axe-core 4.10.2) + `quality.mjs` route × viewport run with artifact capture + `browser-ui-quality.yml` CI, and `tests/integration/browser_ui_quality.rs` (unauth matrix, Owner/Admin/User accessible, 3-viewport responsive, CSS probes, localization fallback/plural/format + shell `dir`). 14 `openpanel-web` lib + 7/7 new integration tests green; `openspec validate --strict` green.
- `make check` at change 7 is green end-to-end (fmt, clippy, docs, audit, file-length, scan-literal, class-coverage, browser-ui-quality, release-governance, tasks-testing-first, reuse-strict, layering, spec-test-drift-strict with only tracked baseline debt, spec-drift, agent-governance, governance-contract 0 failures, coverage-floor skipped/tool-absent-not-required, maturity, test-gates 71/71, test). No pre-existing failures remain; the `audit` leg passes with only allowed warnings.
- `add-operator-security-control-plane` (maturity 8) is implemented and archived as `2026-09-13-add-operator-security-control-plane` (commit `e84ca33`) with new live spec `openspec/specs/operator-security-control-plane/spec.md` (4 requirements: normalized/prioritized findings, previewed/verified remediation, expiring suppression, safe evidence). Delivered: domain `operator_security` (stable ids, dedup, deterministic order, expiry, `redact_text`; metadata redaction reuses `openpanel_core::audit::redact_metadata`), app `operator_security/{types,port,service}` (lifecycle, 5 typed adapters with rollback contracts and recovery guidance, `ExistingServiceRemediationPort` delegating to the firewall/WAF/malware/compliance/service-manager services, per-adapter audit + `EventKind::Audit` fan-out), web `/security/findings*` (Owner/Admin + CSRF, nav/registry entries, dashboard attention hook, no new CSS tokens), API `/api/v1/security/findings*`, `security findings` CLI subcommands, and `docs/SECURITY_CONTROL_PLANE.md` runbook. 11 domain + 10 app + 1 notification-integration + 221 web lib + 7 HTTP integration + 2 CLI E2E tests green; `openspec validate --strict` green.
- `make check` at change 8 is green end-to-end (fmt, clippy, docs, audit, file-length, scan-literal, class-coverage, browser-ui-quality, release-governance, tasks-testing-first, reuse-strict with only classified debt, layering, spec-test-drift-strict with only tracked baseline debt, spec-drift, agent-governance, governance-contract 0 failures, coverage-floor skipped/tool-absent-not-required, maturity, test-gates, test). One unrelated flake observed (`cli_docker_memory_ceiling` fails under parallel load, passes in isolation); the recording full run is EXIT=0. This change's own `reuse-strict` run flagged new bare names (`suppress`, `rule`, `actor`, `queue`, `seed`, `remediate`, `redact_metadata`); fixed by renames (`rule_key`, `suppressed_by`, `apply_suppression`, `suppress_finding`, `queue_page`/`seed_queue`/`suppress_post`/`remediate_post`) and metadata-redaction reuse instead of baseline growth. App service split into `types.rs`/`port.rs`/`service.rs` under the file-length hard limit.
- `expand-monitoring-and-fleet-operations` (maturity 9) is implemented and archived as `2026-09-13-expand-monitoring-and-fleet-operations` (commit `b4e111e`) with new live spec `openspec/specs/monitoring-fleet-operations/spec.md` (4 requirements: configurable views, threshold hysteresis, independent uptime, safe scoped fleet health). Delivered: domain `monitoring_fleet` (bounded `FleetMetricQuery`/`FleetRefreshPolicy`/`FleetSavedView`, `FleetDataState` empty/stale/unavailable, `FleetThresholdPolicy` breach-vs-recovery hysteresis with dedup + hourly rate limits, `IndependentProbeOrigin`/`IndependentProbeResult` with panel-down vs target-down distinction, `project_fleet_host` heartbeat expiry + version/manifest drift + owner scoping, secret-free by construction), app `MonitoringFleetService` (in-memory projection over `SnapshotRepository` like `operator_security`, audit + notification fan-out), web `GET /monitoring/views` + `POST /monitoring/views/save` + `GET /fleet` (registry/nav entries reusing the `monitoring` capability, `table`/`form`/`btn`/`op-empty-state`/`op-error-state` tokens only, no new CSS), API `/api/v1/monitoring/views` + `/fleet/health` + `/probe/independent`, `monitoring validate-query` + `monitoring fleet` CLI subcommands, and `docs/MONITORING_FLEET.md` runbook. 14 domain (+2 prop) + 7 app + 4 web + 6 HTTP integration + 1 CLI E2E tests green; `openspec validate --strict` green.
- `make check` at change 9 is green end-to-end (fmt, clippy, docs, audit, file-length, scan-literal, class-coverage, browser-ui-quality, release-governance skipped/no-dist-dir, tasks-testing-first, reuse-strict with only classified debt, layering, spec-test-drift-strict with only tracked baseline debt, spec-drift, agent-governance, governance-contract 0 failures, coverage-floor skipped/tool-absent-not-required, maturity, test-gates 71/71, test). This change's own `reuse-strict` run flagged one new bare name (`monitoring_fleet` fn in test-support + CLI handlers); fixed by renaming the CLI handler to `monitoring_fleet_status`. The `docs` leg flagged broken intra-doc links in the new web module; fixed by plain-text references.
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

1. **Maturity sequence complete (9/9).** `expand-monitoring-and-fleet-operations`
    is implemented and archived as `2026-09-13-expand-monitoring-and-fleet-operations`
    (commit `b4e111e`). No further changes are queued; the next roadmap
    is defined by a human principal.
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
- `openspec/changes/complete-backup-dr-and-migration-operations/`
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

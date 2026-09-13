# Progress: complete-backup-dr-and-migration-operations

## Status

- Uncommitted-code research (2026-09-13): worktree has no uncommitted
  implementation. `M HANDOFF.md` is ariadex frontmatter only; `.ariadex/`
  is runner state; the five untracked `openspec/changes/*` folders are
  plan-only (proposal/design/tasks/specs, ~1KB each). Moving to next
  spec per user instruction.
- Selected change 5: `complete-backup-dr-and-migration-operations`
  (depends on maturity 1-2, both archived). `openspec validate
  --strict` on the plan passes (checked before implementation).

## Design approval

Standing principal direction (HANDOFF.md, 2026-09-13): design approval
for the remaining maturity changes is pre-granted (automatic approve);
approval recorded here. `design.md` reviewed: health projection over
existing `BackupService`/`DrillService`/snapshot-importer/offsite
adapters; restore stays preflight-token; no new archive format; no
silent destructive restore. Approved for apply.

## Approach

- Domain (pure, I/O-free): `backups/health.rs` (`BackupHealth`,
  `BackupHealthStatus`, `project_backup_health`) and
  `backups/migration.rs` (`MigrationReadiness`,
  `check_manifest_compatibility`, `preview_migration_collisions`,
  `migration_bootstrap_command`, `assess_migration_readiness`).
- App: `backups/backup_health.rs` (`latest_completed_run`,
  `plan_health_from_runs` over existing `BackupPlan`/`BackupRun`
  repos, no new tables) and `backups/remote_verify.rs`
  (`RemoteRoundtrip`, `verify_remote_roundtrip` put/get/list/delete
  probe over `BackupTargetAdapter`).
- Drills: reuse `DrillService` (sandbox teardown, retention 20,
  failure notification, audit); no new tables/routes.
- Surfaces deferred (precedent: `complete-file-database-backup-workflows`):
  on-demand/history drill routes, restore preview/token routes, and
  `export_to_target`/`import_from_target` already exist; streaming
  restore progress UI stays a follow-up. Operator runbook in
  `docs/BACKUP_DR.md`.

## Done

- Domain `backups/health.rs`: `BackupHealth`, `BackupHealthStatus`,
  `project_backup_health` + 6 unit tests + 1 property test.
- Domain `backups/migration.rs`: `MigrationReadiness`,
  `check_manifest_compatibility`, `preview_migration_collisions`,
  `migration_bootstrap_command`, `assess_migration_readiness` + 9
  unit tests + 1 property test.
- App `backups/backup_health.rs`: `latest_completed_run`,
  `plan_health_from_runs` over existing `BackupPlan`/`BackupRun`
  repos (no new tables) + 5 unit tests.
- App `backups/remote_verify.rs`: `RemoteRoundtrip`,
  `verify_remote_roundtrip` bounded put/get/list/delete probe + 7
  unit tests (connectivity, credential rejection, write/read,
  retention, unavailable).
- Integration `crates/openpanel-app/tests/backup_dr_operations.rs`:
  6 tests (health stale/unknown, remote ok/unavailable, migration
  round-trip + preview/commit audit events, failed drill with
  notification wiring + history).
- Docs: `docs/BACKUP_DR.md` runbook (RPO/RTO, drills, scoped
  restore, migration, remote targets).
- Red phase: the new tests were written against modules that did not
  exist yet (compile-fail red) before implementation; green after.

## Scope notes (honest residuals)

- Surfaces reuse existing routes: drill on-demand/history
  (`/runs/{id}/drills`), restore preview/token, `BackupDrillCommand`
  CLI, `export_to_target`/`import_from_target`. Drill scheduling
  reuses the cron capability; no new HTTP/CLI surface was added.
- Streaming restore progress UI is deferred as a follow-up (same
  precedent as `complete-file-database-backup-workflows`).
- Both-hosts audit: the import path is pinned
  (`MigrationPreviewed`/`MigrationRunCommitted` via
  `MigrationService`; drill start/completion via `DrillService`);
  source-side export audit inside `export_to_target` remains a
  follow-up (no signature change made here).

## Verification

- `openspec validate complete-backup-dr-and-migration-operations
  --strict`: valid.
- Focused: `openpanel-domain` lib 545 passed; `openpanel-app`
  `backups::` lib 29 passed; `backup_dr_operations` 6 passed;
  `backup_drills` 4 passed; `notifications` 3 passed + 1 ignored
  (loopback-gated).
- `make check`: green except the pre-existing
  `spec-test-drift-strict` `[FAIL] ssl-production-lifecycle: 12
  scenario(s), no covering test` from commit `7c94228` (no covering
  test shipped with that change; tracked for its follow-up, not
  fixed here per one-change-at-a-time). `backup-dr-operations`
  itself is covered. Later gates verified individually:
  `spec-drift` ok, `agent-governance` ok, `governance-contract` ok
  (0 failures), `coverage-floor` skipped (tool absent, not
  required), `maturity` ok, `test-gates` 71/71.
- Environment-backed restore drill: not run (no staging storage +
  database prerequisites in this environment).

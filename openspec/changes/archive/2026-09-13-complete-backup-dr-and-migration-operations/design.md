# Design: Complete backup DR and migration operations

## Approach

Build on the current `BackupService`, `DrillService`, snapshot importer, and
notification dispatcher. Add an operator-facing health projection rather than
duplicating backup state. Restore remains a preflight-token workflow: inspect,
confirm once, execute in dependency order, stream bounded progress, and retain
an auditable result.

## Explore & Reuse

- Reuse `BackupService`, `DrillService`, `ServerSnapshotService`, and existing
  restore preflight/confirmation types.
- Reuse `SnapshotBundleSource`, `SnapshotImporterDriver`, and offsite target
  adapters.
- Reuse cron scheduling, notifications, audit activity, `TaskState`, and
  `ErrorState` UI components.

## Boundaries

Backup domain owns lifecycle and integrity; app services own provider and
sandbox execution; web/API/CLI expose progress and reports. The health
projection must not become a second source of truth.

## Verification

Use fake remote targets and bounded sandboxes for deterministic tests, then
run a documented environment-backed restore drill separately. Verify cleanup
on every failure path.

## Non-goals

- Replacing the current archive format.
- Cross-host orchestration beyond the declared bootstrap flow.
- Automatic restore without confirmation.
